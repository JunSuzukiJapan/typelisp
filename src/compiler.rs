//! The (typelisp-hosted) compiler body: AST (bridged to `Sexpr` by
//! [`crate::compile::ast_bridge`]) -> LLVM IR, built by calling the
//! `llvm-*` builtins (`crate::eval::interp`'s `eval_llvm_builtin_method`)
//! directly — the same "Rust provides the bindings, typelisp drives them"
//! split [docs/implementation-log.md](../docs/dev/implementation-log.md) calls for. Loaded the same way
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
//! treating it as "the value"; `compile-set`/`compile-cellset` are the only
//! places that write to an existing slot a second time (or, for a cell-kind
//! name, to the cell itself — see just below). A `setf` on a *captured*
//! name now genuinely shares — closure-representation unification, Stage 4:
//! any name `ast_bridge::names_captured_by_nested` finds referenced inside a
//! nested `lambda`/`labels` is promoted, at `bind-params`/`bind-let-values`
//! time, to a shared `BoxedObj::Cell` (`rt_cell_new`) instead of an ordinary
//! stack slot — a captured name is *always* one of these (every entry a
//! `labels`/`lambda`'s own shared captured-list can ever hold is cell-kind
//! unconditionally, since Stage 4 doesn't yet narrow this to only the names
//! actually reassigned somewhere — a deliberately deferred optimization).
//! `compile-cellvar`/`compile-cellset` read/write through the cell
//! (`rt_cell_get`/`rt_cell_set`) rather than the slot itself, so every
//! holder of that same cell — an outer activation's own binding, a sibling
//! closure that also captured it, even the tree-walking interpreter's own
//! `Slot::Heap` if this cell originated there — sees the write immediately,
//! matching the interpreter's own true shared-cell closures (heap-cell
//! captures, see `BoxedObj::Closure`/`ClosureBody`) exactly. The one
//! exception: a `labels` sibling captured *as a value* (not called) is
//! never cell-boxed even though it appears in the very same captured-list —
//! see `Ctx::visible_siblings`'s doc comment (`ast_bridge.rs`) for why a
//! sibling reference has no sharable mutable state to begin with.
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
//! Every captured/parameter name and call argument carries a `kind` `Int`
//! tag alongside it (`ast_bridge`'s `tagged_sym_list`/`tagged_ast_list_to_sexpr`,
//! `binding_kind`'s doc comment) — `2` (`Sexpr`) for a `Sexpr`-, `Str`-, or
//! `Fn`-typed name/argument, `0` (plain) otherwise. This is what
//! `retain-bindings`/`release-bindings`/`compile-call-args` key their
//! GC-root push/pop on (see those functions' doc comments for the exact
//! insertion points): a `kind = 2` name gets a root pushed at its binding
//! site and popped at its activation's exit, so an allocation anywhere in
//! between can't reclaim it out from under a still-live binding. A `Fn`-typed
//! value used to need a *different*, ARC-based scheme instead (retain on
//! capture/entry, release on exit/post-call — labels/closures Stage 4) to
//! protect its own `ClosureBox`, a raw `malloc`'d block the GC couldn't see
//! at all; the closure-representation unification retired that scheme
//! entirely once a compiled closure became an ordinary GC-heap
//! `BoxedObj::CompiledClosure` — see `crate::mem::BoxedObj`'s own doc
//! comment. No value crossing any binding boundary here needs ownership
//! bookkeeping beyond that root push/pop: a GC-traced value can be
//! referenced from any number of places at once with nothing to double-free.
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
//! (`retain-bindings`/`release-bindings`/`name-is-borrowed?`/
//! `form-is-borrowed?`/`compute-sexpr-mask`/...) stay ordinary top-level
//! `defun`s, defined before
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
//! `compile-if-branch` is a thin wrapper around `compile-value` shared by
//! `if`/`return`/`set`/`match`'s own arm-compiling helpers — before the
//! closure-representation unification it also retained a *borrowed*,
//! `Fn`-typed branch value right there (an `if` merge slot, unlike a call/
//! `labels`/`lambda` boundary, introduces no function activation of its own
//! to freshen an ARC-owned value at). A GC-traced value needs no such
//! freshening — it just flows through — so that step is gone; see
//! `compile-if-branch`'s own doc comment.
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
//! `Scope`'s methods, `Option`'s `some`/`none`/pattern-matching), so
//! loading order relative to the prelude doesn't matter.

use crate::{Checker, Heap, Interp, Reader};

pub const SOURCE: &str = r#"
;; `sexpr-str`/`sexpr-bool`/`sexpr-sym-name`/`sexpr-int` — the island's typed
;; `Sexpr` field extractors — are now Rust builtins (`Interp::eval_builtin`,
;; registered in `check::registry`), not `match`-based typelisp defuns.
;; Symbol/Sexpr redesign Phase 2 moved them out of this source so the island
;; navigates `Sexpr` structure entirely through the `sexpr-*` layer, never the
;; user-facing `match` (Phase 5 fences `match` to enum scrutinees). Each still
;; panics on a tag mismatch, exactly as the old `(_ (panic ...))` arms did.

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
;; reopening the exact GC-safety gap Stage 4/6 closed for `Sexpr`. `float`'s
;; `f64` field (Sexpr/RtValue unification, Stage 0) goes through
;; `rt_float_value` -- a real heap read, like `cons`, since a float is now
;; boxed rather than an immediate bit pattern (see `typelisp-rt`'s
;; `TAG_BOXED`). `sym`'s `Symbol` field still isn't representable in
;; compiled code (a `Sym`'s tagged payload is a `SymId`; `Type::Symbol` has
;; no compiled representation, and `Interp::call_compiled`'s return-value
;; decode would degrade one to a plain `Int` at the JIT boundary), and
;; neither are `bignum`/`ratio`'s payloads (`Type::Bignum`/`Type::Ratio`
;; are heap objects with no `rt_bignum_*`/`rt_ratio_*` support yet — see
;; `ast_bridge`'s `Expr::Bignum` note), so a `Bind` pattern trying to
;; extract any of these panics clearly here rather than producing garbage --
;; never reached for a `Wildcard` sub-pattern (`compile-ctor-subpatterns`
;; skips the call entirely then), so tag-only dispatch on `(sym _)`/
;; `(bignum _)`/`(ratio _)` arms still compiles fine.
;;
;; Relocated ahead of `bind-params` (closure-representation unification,
;; Stage 4): that function's own new `kind >= 10` cell branch needs to call
;; this and `compile-tag-struct-field` below, and this island has no forward
;; declarations — a `defun` can only call a name already defined earlier in
;; this same source string (see this module's doc comment's "Helpers that
;; only ever call *out* of the ring" paragraph for the general shape of this
;; constraint). Purely a textual move; neither function's own body changed.
(defun compile-sexpr-field ((builder llvm-builder) (m llvm-module) (v llvm-value) (variant i64) (idx i32)) llvm-value
  (if (eq variant 1)
      (build-ashr builder v (const-i64 builder 3))
      (if (eq variant 2)
          (let ((args-ptr (alloca-args builder 1)))
            (store-arg builder args-ptr 0 v)
            (build-call builder (get-function m "rt_float_value") args-ptr 1))
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
                      (if (eq variant 5)
                          (panic "compile-sexpr-field: a sym's Symbol payload is not representable in compiled code yet; match it with (sym _) instead of binding it")
                          ;; `bignum`(8)/`ratio`(9): the "payload" *is* the
                          ;; already-tagged boxed value itself (no separate
                          ;; scalar to unwrap the way int/char/bool have) —
                          ;; same passthrough as `str`(6).
                          (if (if (eq variant 8) true (eq variant 9))
                              v
                              (panic "compile-sexpr-field: field type is not representable in compiled code yet"))))))))))

;; The encode-side mirror of `compile-sexpr-field`'s decode, over the exact
;; same `ast_bridge::struct_field_kind`/`Sexpr`-variant numbering (`1`=int
;; `2`=float `3`=char `4`=bool `6`=str/`Sexpr`/nested-boxed-struct/`Fn`
;; passthrough — a `defstruct`/`Vector<T>`/`cons-cell<K,V>`/closure-typed
;; field's value is already a properly tagged `Sexpr`, so it passes through
;; unchanged exactly like a `Str` — `0`=not representable yet: a
;; general-ADT/still-generic field) — `compile-construct-boxed-struct-fields`/
;; `compile-field-set` both call this to turn an already-compiled field
;; value `v` (a scalar kind's own untagged bit pattern, or an already-tagged
;; `Sexpr` for kind `6`) into the properly tagged `Sexpr` `rt_struct_new`/
;; `rt_struct_field_set` require (Stage 3 of the Sexpr/RtValue unification
;; plan, `docs/implementation-log.md`). Mirrors `compile-construct-sexpr`'s
;; own per-variant `int`/`float`/`char`/`bool`/`str` arms bit-for-bit rather
;; than calling into that function directly, since here `v` is already a
;; compiled value (there is no `arg-forms` sub-expression left to compile) --
;; see that function's own doc comment for why each shift/tag constant is
;; what it is.
(defun compile-tag-struct-field ((builder llvm-builder) (m llvm-module) (v llvm-value) (kind i64)) llvm-value
  (if (eq kind 1)
      (build-shl builder v (const-i64 builder 3))
      (if (eq kind 2)
          (let ((args-ptr (alloca-args builder 1)))
            (store-arg builder args-ptr 0 v)
            (build-call builder (get-function m "rt_float_new") args-ptr 1))
          (if (eq kind 3)
              (build-or builder (build-shl builder v (const-i64 builder 3)) (const-i64 builder 4))
              (if (eq kind 4)
                  (build-or builder (build-shl builder (build-add builder v (const-i64 builder 1)) (const-i64 builder 3)) (const-i64 builder 6))
                  (if (eq kind 6)
                      v
                      (panic "compile-tag-struct-field: field type is not representable in compiled code yet")))))))

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
;;
;; `kind >= 10` (closure-representation unification, Stage 4 — see
;; `ast_bridge::tagged_sym_list`'s doc comment for the `10 + struct_field_kind`
;; numbering): this parameter is captured by some closure nested in its own
;; function body, so it must be a shared `BoxedObj::Cell` (`rt_cell_new`), not
;; a plain stack slot — a `setf` inside the capturing closure has to be
;; visible here, and vice versa. The incoming raw argument word is first
;; tagged per `kind - 10` (`compile-tag-struct-field`, the same encode a
;; struct field's own write already uses) since `rt_cell_new` expects an
;; already-tagged `Sexpr`; the tagged value is transiently rooted around that
;; one call (`push-sexpr-root`/`pop-sexpr-root`) since tagging a `kind = 2`
;; (`f64`) payload allocates a fresh `rt_float_new` box that would otherwise
;; sit one GC away from being reclaimed before `rt_cell_new` gets to copy it
;; in. The resulting cell reference is then given its own *permanent* root —
;; pushed here and never popped by this function, unlike the transient one
;; just above — because `retain-bindings`'s own later pass (this function's
;; caller always runs it right after) only ever pushes for `kind = 2`, never
;; `kind >= 10`: a cell must be protected from the very *first* instant it
;; exists, not just once every name in `names` has been bound. Left for
;; `retain-bindings` to push instead, a second cell-kind param bound moments
;; later could allocate (its own `rt_cell_new`) and trigger a GC that
;; reclaims the *first* one, still sitting unrooted in its own slot at that
;; point — this permanent push closes exactly that window.
;; `release-bindings` pops it at function exit, alongside every `kind = 2`
;; root `retain-bindings` pushed — see that function's own doc comment for
;; why popping order among a same-activation batch never matters.
(defun bind-params ((env Scope<llvm-value>) (builder llvm-builder) (m llvm-module) (f llvm-function) (names Sexpr) (idx i32)) ()
  (if (sexpr-consp names)
      (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
        (let ((nm (sexpr-sym-name (sexpr-car name-pair))) (kind (sexpr-int (sexpr-cdr name-pair))))
          (let ((raw (load-arg builder f idx)))
            (let ((slot (alloca-args builder 2)))
              (if (>= kind 10)
                  (let ((tagged (compile-tag-struct-field builder m raw (- kind 10))))
                    (let ((tag-args-ptr (alloca-args builder 1)))
                      (store-arg builder tag-args-ptr 0 tagged)
                      (let ((ignored1 (build-call builder (get-function m "rt_push_sexpr_root") tag-args-ptr 1)))
                        (let ((cell-args-ptr (alloca-args builder 1)))
                          (store-arg builder cell-args-ptr 0 tagged)
                          (let ((cell-ref (build-call builder (get-function m "rt_cell_new") cell-args-ptr 1)))
                            (let ((ignored2 (build-call builder (get-function m "rt_pop_sexpr_root") (alloca-args builder 0) 0)))
                              (let ((root-args-ptr (alloca-args builder 1)))
                                (store-arg builder root-args-ptr 0 cell-ref)
                                (let ((ignored3 (build-call builder (get-function m "rt_push_sexpr_root") root-args-ptr 1)))
                                  (store-arg builder slot 0 cell-ref)))))))))
                  (store-arg builder slot 0 raw))
              (set env nm slot)))
          (bind-params env builder m f rest (+ idx 1))))
      ()))

;; The captures counterpart of `bind-params` (labels/closures Stage 2):
;; reads each captured name back out of a function's env array (`load-env`,
;; the env-array analogue of `load-arg`) into a fresh slot in `env` under its
;; own name (see `bind-params`'s doc comment for why a slot, not the value
;; directly), so `compile-var`'s ordinary by-name lookup finds it exactly
;; like a regular parameter. A no-op when `names` is empty — the Stage 1
;; (no-capture) path this leaves untouched.
;;
;; Every entry `names` (`captured`/`lcaptured`) can ever hold is `kind >= 10`
;; now (closure-representation unification, Stage 4 — `ast_bridge`'s shared
;; captured-list builder tags *every* capture as cell-boxed, unconditionally
;; — see `Ctx::cell_names`'s doc comment), so unlike `bind-params` there is no
;; plain-value branch to keep: the value copied out of the env array is
;; already a valid cell reference (whatever produced this closure's env array
;; — `compile-escaping-env-args`/`compile-env-args` — put it there without
;; dereferencing), so no tagging or `rt_cell_new` call is needed here, just
;; the same permanent `push-sexpr-root` `bind-params` gives a *freshly made*
;; cell — this one protects the *copy* in this activation's own slot, for the
;; identical "protect it before the next capture in this same batch might
;; allocate" reason (a captured name is never rooted purely by inheriting the
;; closure box's own rooting, since the copy lives in a plain stack slot, not
;; scanned by the GC).
(defun bind-captures ((env Scope<llvm-value>) (builder llvm-builder) (m llvm-module) (f llvm-function) (names Sexpr) (idx i32)) ()
  (if (sexpr-consp names)
      (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
        (let ((nm (sexpr-sym-name (sexpr-car name-pair))) (kind (sexpr-int (sexpr-cdr name-pair))))
          (let ((slot (alloca-args builder 2)))
            (let ((v (load-env builder f idx)))
              (store-arg builder slot 0 v)
              (if (if (eq kind 2) true (>= kind 10))
                  (let ((args-ptr (alloca-args builder 1)))
                    (store-arg builder args-ptr 0 v)
                    (let ((ignored (build-call builder (get-function m "rt_push_sexpr_root") args-ptr 1))) ()))
                  ()))
            (set env nm slot))
          (bind-captures env builder m f rest (+ idx 1))))
      ()))

;; 2^n by repeated doubling — `compute-sexpr-mask`'s one helper, not a
;; generic utility; `n` (a captured-slot index) is always small in practice
;; so the O(n) recursion is in no way a bottleneck.
(defun pow2 ((n i32)) i64
  (if (eq n 0) 1 (* (pow2 (- n 1)) 2)))

;; Computes `build-make-closure`'s `sexpr_mask` argument (formerly
;; `fn_mask`): a bitmask, one bit per captured slot, set wherever that
;; slot's `kind` tag (`ast_bridge::binding_kind` — `0`=plain, `2`=sexpr;
;; Stage 6 of the Sexpr-representation plan generalized this from a plain
;; `is-fn` `Bool`, see `tagged_sym_list`'s doc comment) is exactly `2`
;; (sexpr) — entirely a *host*-level computation (no LLVM IR involved,
;; unlike everything `compile-*` builds): the mask is fully determined by
;; `names`' own tags, known already at the point this runs, so there's
;; nothing to generate code for. Every `Fn`-typed capture is itself `kind =
;; 2` now (the closure-representation unification folded it into
;; `KIND_SEXPR` — see `binding_kind`'s doc comment), so this mask is what
;; tells `rt_closure_new` which captured slots are tagged `Sexpr` values the
;; GC mark phase must trace, versus a `0` (plain) slot's raw native bits.
(defun compute-sexpr-mask ((names Sexpr) (idx i32)) i64
  (if (sexpr-consp names)
      (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
        (let ((kind (sexpr-int (sexpr-cdr name-pair))))
          (if (eq kind 2)
              (+ (pow2 idx) (compute-sexpr-mask rest (+ idx 1)))
              (compute-sexpr-mask rest (+ idx 1)))))
      0))

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
  (if (sexpr-symp (sexpr-car form))
      (if (equal (sexpr-sym-name (sexpr-car form)) "var")
          (name-is-borrowed? env (sexpr-str (sexpr-car (sexpr-cdr form))))
          false)
      false))

;; R1 (function entry, design notes): pushes a GC root for every `kind = 2`
;; name in `names` (a tagged params or captured list, as just bound by
;; `bind-params`/`bind-captures`) — a `Sexpr`-, `Str`-, or (since the
;; closure-representation unification) `Fn`-typed parameter/capture is a
;; tagged `i64` that may point into the GC-managed heap, so without this, any
;; allocation anywhere later in this activation's body (a `cons`, another
;; `rt_*` call — including the `rt_closure_new` a nested `lambda`/`labels`
;; sibling might build) could trigger a GC that reclaims it out from under a
;; still-live binding (`typelisp-rt`'s `rt_push_sexpr_root` doc comment/tests
;; demonstrate exactly this failure mode). `release-bindings`, at this same
;; activation's exit, undoes it. A `Fn`-typed name used to get its own
;; `ClosureBox` refcount-retain here instead (`kind = 1`, labels/closures
;; Stage 4) — retired along with the rest of the ARC scheme once a compiled
;; closure became an ordinary GC-heap value with nothing to refcount; every
;; capture/parameter this function ever sees is `kind = 0` or `kind = 2` now
;; (`binding_kind`'s doc comment), so a single `(eq kind 2)` test covers it.
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
  (if (sexpr-consp names)
      (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
       (let ((nm (sexpr-sym-name (sexpr-car name-pair))))
       (let ((kind (sexpr-int (sexpr-cdr name-pair))))
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
             ())
         (retain-bindings builder m env rest))))
      ()))

;; R2 (function exit, design notes): pops the GC root `retain-bindings`
;; pushed for every `kind = 2` name in `names` — unconditionally: popping a
;; root never frees anything (it only stops a GC root walk from visiting
;; that slot), so even a name leaving as this activation's own bare return
;; value has its root popped here just the same — the tagged `i64` itself
;; still flows out via this activation's own `build-ret` regardless, and if
;; its *caller* needs to keep it alive across further allocations, that's
;; the caller's own binding (a `let`, another function's params) that pushes
;; a fresh root for it, exactly as it would for any other fresh `Sexpr`
;; value. Leaving this activation's own push on the stack past its own
;; lifetime would otherwise grow `Heap`'s root stack without bound across
;; repeated calls, eventually desyncing every later `rt_pop_sexpr_root`'s
;; LIFO assumption. A `Fn`-typed name used to need an exception here (a
;; `protected` name, carved out of an unconditional `ClosureBox`
;; refcount-release — labels/closures Stage 4): retired along with the rest
;; of the ARC scheme, since a GC root pop was never conditional on ownership
;; to begin with — see `retain-bindings`'s doc comment for why every capture/
;; parameter is `kind = 0` or `kind = 2` now.
;;
;; `kind >= 10` (Stage 4) also pops one — not because *this* function pushed
;; it (unlike `kind = 2`, always `retain-bindings`'s own), but because
;; whichever of `bind-params`/`bind-captures` bound this name pushed exactly
;; one permanent root for it directly (see their own doc comments for why the
;; push has to happen immediately at bind time, not deferred here the way
;; `kind = 2`'s is) — `release-bindings` is where every binding's root, no
;; matter which function pushed it, is popped in one place at function exit.
(defun release-bindings ((builder llvm-builder) (m llvm-module) (env Scope<llvm-value>) (names Sexpr)) ()
  (if (sexpr-consp names)
      (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
       (let ((kind (sexpr-int (sexpr-cdr name-pair))))
         (if (if (eq kind 2) true (>= kind 10))
             (let ((args-ptr (alloca-args builder 0)))
               (let ((ignored (build-call builder (get-function m "rt_pop_sexpr_root") args-ptr 0))) ()))
             ())
         (release-bindings builder m env rest)))
      ()))

;; The bare `rt_push_sexpr_root`/`rt_pop_sexpr_root` call `retain-bindings`/
;; `release-bindings`/`bind-let-values` each inline for a *named* `kind = 2`
;; binding (Stage 6 of the Sexpr-representation plan,
;; `docs/implementation-log.md`), factored out for a *fresh*, unnamed `Sexpr` value instead
;; (a `Cons`'s own car/cdr sub-expression, a call argument) — Stage 8's own
;; residual gap, the "残る選択肢" entry that gap left for a later pass:
;; a temporary `Sexpr` value sitting only in a raw stack slot (never bound
;; to a name) has no GC root at all, so any allocation made while computing
;; a *sibling* operand (another field, another argument) before the value is
;; finally consumed (a `cons`, a call) could have it reclaimed out from
;; under that slot. `push-sexpr-root` returns nothing — a GC root-stack push
;; has no return value of its own worth threading through.
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

;; `compile-let`'s own `acc` — `compile-let-values`'s scratch accumulator of
;; each binding's freshly computed value, keyed by name. This is never
;; pushed/popped as a frame of its own (it's discarded the moment
;; `bind-let-values` has read every value back out of it into `env`'s real
;; new frame); a single-frame `Scope` (rather than a plain `HashTable`, which
;; Stage 5 of the Sexpr/RtValue unification moved onto the GC-managed cons
;; heap's boxed-struct representation — `crate::mem::Value` can't hold an
;; opaque `llvm-value` handle, unlike this `RtValue`-native `Scope`) is what
;; gives `acc` a place to live that can — see `new-env`'s comment for why
;; this still needs its own tiny wrapper rather than an inline
;; `(Scope::new)`.
(defun new-acc-table () Scope<llvm-value> (Scope::new))

;; `compile-assoc`'s native-method dispatch predicates: is `method` one of
;; the receiver-type methods that lower to LLVM instructions (i64/i32) or
;; `rt_str_*` primitive calls (string) rather than a real function call?
;; Anything *not* on these lists — a user-defined method on a primitive
;; receiver, e.g. the prelude's `impl Eq i32` → `equals` — falls through to
;; `compile-assoc-user`'s ordinary mangled-name call. (Chained `if`s: the
;; prelude's `or` macro isn't loaded under `run_with_compiler`.)
(defun int-native-method? ((method string)) bool
  (if (equal method "+") true
  (if (equal method "-") true
  (if (equal method "*") true
  (if (equal method "<") true
  (if (equal method "<=") true
  (if (equal method ">") true
  (if (equal method ">=") true
  (if (equal method "=") true
  (if (equal method "eq") true
  (if (equal method "/=") true
  ;; `int->bignum`/`int->ratio`: always-exact widening into the two
  ;; arbitrary-precision types (`rt_int_to_bignum`/`rt_int_to_ratio`) — unary,
  ;; checked before `b2` the same way `float-native-method?`'s own unary
  ;; conversions are.
  (if (equal method "int->bignum") true
  (equal method "int->ratio")))))))))))))

(defun string-native-method? ((method string)) bool
  (if (equal method "length") true
  (if (equal method "ref") true
  (if (equal method "eq") true
  (if (equal method "equal") true
  (if (equal method "equalp") true
  (if (equal method "lt") true
  (if (equal method "<") true
  (if (equal method "<=") true
  (if (equal method ">") true
  (if (equal method ">=") true
  (equal method "append"))))))))))))

;; `char`'s natively-compilable methods: a compiled `char` is a raw `i64`
;; code point, so the content comparisons lower to the same integer `icmp`s
;; the int branch uses. `equalp` (ASCII case-insensitive) has no single
;; instruction and stays non-native. `char->int` is the identity at the
;; compiled level (a `char`'s value *is* its code point, and `i32`/`i64`
;; share width) — needed so the island's own `compile-char` (which calls
;; `(char->int (sexpr-char ...))`) is itself compilable.
(defun char-native-method? ((method string)) bool
  (if (equal method "eq") true
  (if (equal method "eql") true
  (if (equal method "equal") true
  (if (equal method "equalp") true
  (if (equal method "lt") true
  (if (equal method "<") true
  (if (equal method "<=") true
  (if (equal method ">") true
  (if (equal method ">=") true
  (if (equal method "char->int") true
  false)))))))))))

;; `f64`'s natively-compilable methods: arithmetic (`+`/`-`/`*`/`/`/`mod`)
;; lowers to LLVM float instructions (`build-fadd`/... — each `bitcast`s the
;; `i64`-carried `f64` bits to `double` and back internally), comparisons
;; (`<`/`<=`/`>`/`>=`/`=`/`/=` and the `eq`/`eql`/`equal`/`equalp` aliases) to
;; `build-fcmp-*`. `expt` and the unary `sqrt`/`floor`/`ceiling`/`round`/
;; `truncate` family lower to the corresponding LLVM intrinsic (`build-fpow`/
;; `build-fsqrt`/...), and `float->int` to a single `fptosi` instruction
;; (`build-fptosi`) — all native, same as the arithmetic ops. `float->bignum`/
;; `float->ratio` are unary too (`rt_float_to_bignum`/`rt_float_to_ratio`),
;; now that `bignum`/`ratio` have a compiled representation.
(defun float-native-method? ((method string)) bool
  (if (equal method "+") true
  (if (equal method "-") true
  (if (equal method "*") true
  (if (equal method "/") true
  (if (equal method "mod") true
  (if (equal method "expt") true
  (if (equal method "sqrt") true
  (if (equal method "floor") true
  (if (equal method "ceiling") true
  (if (equal method "round") true
  (if (equal method "truncate") true
  (if (equal method "float->int") true
  (if (equal method "float->bignum") true
  (if (equal method "float->ratio") true
  (if (equal method "<") true
  (if (equal method "<=") true
  (if (equal method ">") true
  (if (equal method ">=") true
  (if (equal method "=") true
  (if (equal method "/=") true
  (if (equal method "eq") true
  (if (equal method "eql") true
  (if (equal method "equal") true
  (if (equal method "equalp") true
  false)))))))))))))))))))))))))

;; Emits `rt_str_lt(x, y)` (strict lexicographic less-than, an `i64` 0/1). The
;; four string comparison operators all derive from it: `<`=lt(a,b),
;; `>`=lt(b,a), `<=`=not lt(b,a), `>=`=not lt(a,b) — so no `rt_str_le`/`_gt`/
;; `_ge` runtime helpers are needed (see `compile-assoc`'s string branch).
(defun str-lt-call ((builder llvm-builder) (m llvm-module) (x llvm-value) (y llvm-value)) llvm-value
  (let ((args-ptr (alloca-args builder 2)))
    (store-arg builder args-ptr 0 x)
    (store-arg builder args-ptr 1 y)
    (build-call builder (get-function m "rt_str_lt") args-ptr 2)))

;; `bignum` (`registry::bignum_assoc`)'s natively-compilable methods: a
;; `bignum` receiver is always an already-tagged boxed value (like `str`, no
;; bit manipulation needed), so arithmetic/comparison lower to a single
;; `rt_bignum_*` call each. `eq`/`eql`/`equal`/`equalp` are aliases for `=`
;; (`bignum_assoc`'s own doc comment — both operands are always `bignum`
;; here). `bignum->int`/`try-bignum->int`/`bignum->float`/`bignum->ratio`
;; round out the conversions.
(defun bignum-native-method? ((method string)) bool
  (if (equal method "+") true
  (if (equal method "-") true
  (if (equal method "*") true
  (if (equal method "/") true
  (if (equal method "mod") true
  (if (equal method "<") true
  (if (equal method "<=") true
  (if (equal method ">") true
  (if (equal method ">=") true
  (if (equal method "=") true
  (if (equal method "/=") true
  (if (equal method "eq") true
  (if (equal method "eql") true
  (if (equal method "equal") true
  (if (equal method "equalp") true
  (if (equal method "bignum->int") true
  (if (equal method "try-bignum->int") true
  (if (equal method "bignum->float") true
  (equal method "bignum->ratio"))))))))))))))))))))

;; `ratio` (`registry::ratio_assoc`)'s natively-compilable methods — the
;; `ratio` counterpart of [`bignum-native-method?`] (no `mod`, CL doesn't
;; define a rational remainder), plus `ratio->bignum`/`ratio->float`/
;; `numerator`/`denominator`.
(defun ratio-native-method? ((method string)) bool
  (if (equal method "+") true
  (if (equal method "-") true
  (if (equal method "*") true
  (if (equal method "/") true
  (if (equal method "<") true
  (if (equal method "<=") true
  (if (equal method ">") true
  (if (equal method ">=") true
  (if (equal method "=") true
  (if (equal method "/=") true
  (if (equal method "eq") true
  (if (equal method "eql") true
  (if (equal method "equal") true
  (if (equal method "equalp") true
  (if (equal method "ratio->bignum") true
  (if (equal method "ratio->float") true
  (if (equal method "numerator") true
  (equal method "denominator")))))))))))))))))))

;; Emits `rt_bignum_cmp(x, y)` (three-way `-1`/`0`/`1`, `BigInt::cmp`) — every
;; bignum comparison operator derives from it via a single `icmp` against
;; `0`, the same "one primitive, several derived comparisons" shape
;; `str-lt-call` establishes for strings (`compile-assoc`'s bignum branch).
(defun bignum-cmp-call ((builder llvm-builder) (m llvm-module) (x llvm-value) (y llvm-value)) llvm-value
  (let ((args-ptr (alloca-args builder 2)))
    (store-arg builder args-ptr 0 x)
    (store-arg builder args-ptr 1 y)
    (build-call builder (get-function m "rt_bignum_cmp") args-ptr 2)))

;; `ratio` counterpart of [`bignum-cmp-call`] (`rt_ratio_cmp`).
(defun ratio-cmp-call ((builder llvm-builder) (m llvm-module) (x llvm-value) (y llvm-value)) llvm-value
  (let ((args-ptr (alloca-args builder 2)))
    (store-arg builder args-ptr 0 x)
    (store-arg builder args-ptr 1 y)
    (build-call builder (get-function m "rt_ratio_cmp") args-ptr 2)))

;; Shared two-operand `rt_bignum_*` call shape (`+`/`-`/`*`/`/`/`mod`) —
;; `fname` names which primitive (`compile-assoc`'s bignum branch).
(defun bignum-binop-call ((builder llvm-builder) (m llvm-module) (fname string) (x llvm-value) (y llvm-value)) llvm-value
  (let ((args-ptr (alloca-args builder 2)))
    (store-arg builder args-ptr 0 x)
    (store-arg builder args-ptr 1 y)
    (build-call builder (get-function m fname) args-ptr 2)))

;; Shared one-operand `rt_bignum_*` call shape (the conversions:
;; `bignum->int`/`bignum->float`/`bignum->ratio`).
(defun bignum-unary-call ((builder llvm-builder) (m llvm-module) (fname string) (x llvm-value)) llvm-value
  (let ((args-ptr (alloca-args builder 1)))
    (store-arg builder args-ptr 0 x)
    (build-call builder (get-function m fname) args-ptr 1)))

;; `ratio` counterpart of [`bignum-binop-call`].
(defun ratio-binop-call ((builder llvm-builder) (m llvm-module) (fname string) (x llvm-value) (y llvm-value)) llvm-value
  (let ((args-ptr (alloca-args builder 2)))
    (store-arg builder args-ptr 0 x)
    (store-arg builder args-ptr 1 y)
    (build-call builder (get-function m fname) args-ptr 2)))

;; `ratio` counterpart of [`bignum-unary-call`] (`ratio->bignum`/
;; `ratio->float`/`numerator`/`denominator`).
(defun ratio-unary-call ((builder llvm-builder) (m llvm-module) (fname string) (x llvm-value)) llvm-value
  (let ((args-ptr (alloca-args builder 1)))
    (store-arg builder args-ptr 0 x)
    (build-call builder (get-function m fname) args-ptr 1)))

;; Counts a plain `Sexpr` list's elements — used to size the `i64*` args
;; array a direct call needs (`compile-apply`'s `alloca-args`/`build-call`),
;; and (labels/closures Stage 2) the `i64*` env array a captured call needs.
(defun sexpr-list-length ((s Sexpr)) i32
  (if (sexpr-consp s)
      (+ 1 (sexpr-list-length (sexpr-cdr s)))
      0))

;; The `i64` counterpart of `sexpr-list-length`, needed wherever the count
;; itself must become a real `const-i64` runtime value rather than only a
;; host-side `alloca-args`/`store-arg`/`load-raw` slot-index constant — the
;; language has no `i32`-to-`i64` widening cast, so `sexpr-list-length`'s own
;; `i32` result can't simply be reused for that. `compile-field-get`/
;; `compile-field-set` (Stage 3 of the Sexpr/RtValue unification plan,
;; `docs/implementation-log.md`) use this to recover `idx-unary-list`'s
;; length as the raw field-index argument `rt_struct_field_get`/
;; `rt_struct_field_set` expect.
(defun sexpr-list-length-i64 ((s Sexpr)) i64
  (if (sexpr-consp s)
      (+ (sexpr-list-length-i64 (sexpr-cdr s)) 1)
      0))

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
;;
;; `kind >= 10` (closure-representation unification, Stage 4): same cell
;; promotion `bind-params`'s own `kind >= 10` branch does, for the identical
;; reason (this specific `let`-bound name is captured by a closure nested in
;; its own scope) — tag `v` per `kind - 10`, allocate a cell holding it, and
;; give the *cell reference* a permanent root immediately (not deferred to
;; any later pass, and not recorded in the slot's offset 1 either: unlike a
;; `kind = 2` binding, a cell-kind slot's own word 0 never changes after this
;; — `compile-cellset` mutates the cell in place, never the slot — so there
;; is no root-index for a later `setf` to update).
(defun bind-let-values ((builder llvm-builder) (m llvm-module) (env Scope<llvm-value>) (bindings Sexpr) (acc Scope<llvm-value>)) ()
  (if (sexpr-consp bindings)
      (let ((pair (sexpr-car bindings)) (rest (sexpr-cdr bindings)))
       (let ((name-pair (sexpr-car pair)))
       (let ((nm (sexpr-sym-name (sexpr-car name-pair))))
         (let ((kind (sexpr-int (sexpr-cdr name-pair))))
           (match (get acc nm)
             ((Some v) (let ((slot (alloca-args builder 2)))
                         (if (>= kind 10)
                             (let ((tagged (compile-tag-struct-field builder m v (- kind 10))))
                               (push-sexpr-root builder m tagged)
                               (let ((args-ptr (alloca-args builder 1)))
                                 (store-arg builder args-ptr 0 tagged)
                                 (let ((cell-ref (build-call builder (get-function m "rt_cell_new") args-ptr 1)))
                                   (pop-sexpr-root builder m)
                                   (push-sexpr-root builder m cell-ref)
                                   (store-arg builder slot 0 cell-ref))))
                             (let ((ignored (store-arg builder slot 0 v)))
                               (if (eq kind 2)
                                   (let ((root-idx (build-call builder (get-function m "rt_root_count") (alloca-args builder 0) 0)))
                                     (store-arg builder slot 1 root-idx)
                                     (let ((args-ptr (alloca-args builder 1)))
                                       (store-arg builder args-ptr 0 v)
                                       (let ((ignored2 (build-call builder (get-function m "rt_push_sexpr_root") args-ptr 1))) ())))
                                   ())))
                         (set env nm slot)))
             (None (panic "bind-let-values: missing computed value")))
           (bind-let-values builder m env rest acc)))))
      ()))

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
;; `kind >= 10` (Stage 4) pops one too, matching `release-bindings`'s own
;; broadened condition — `bind-let-values`'s cell branch pushes its cell
;; reference's root permanently at bind time, so it needs the same unwind
;; here as a `kind = 2` binding's does.
(defun unroot-let-sexpr-values ((builder llvm-builder) (m llvm-module) (bindings Sexpr)) ()
  (if (sexpr-consp bindings)
      (let ((pair (sexpr-car bindings)) (rest (sexpr-cdr bindings)))
       (let ((kind (sexpr-int (sexpr-cdr (sexpr-car pair)))))
         (if (if (eq kind 2) true (>= kind 10))
             (let ((args-ptr (alloca-args builder 0)))
               (let ((ignored (build-call builder (get-function m "rt_pop_sexpr_root") args-ptr 0))) ()))
             ())
         (unroot-let-sexpr-values builder m rest)))
      ()))

;; Tests whether tagged Sexpr value `v` is Sexpr variant `variant`
;; (`registry::sexpr_def`'s variant order: 0=nil 1=int 2=float 3=char
;; 4=bool 5=sym 6=str 7=cons 8=bignum 9=ratio) -- `nil`/`bool` both compile
;; to the same 3-bit tag (`typelisp-rt`'s `TAG_IMMEDIATE`, 6) and are told
;; apart by `v`'s payload bits instead (`0` vs non-zero, see that crate's
;; `encode`/`decode`). `float`/`bignum`/`ratio` all share `TAG_BOXED` (7)
;; with every other heap-boxed object, so the tag alone can't tell them
;; apart -- they go through `rt_box_kind` (`1`/`2`/`3`; a match already
;; implies the tag is `TAG_BOXED`, so no separate tag check is emitted).
;; Every remaining variant has its own dedicated tag.
(defun compile-sexpr-tag-test ((builder llvm-builder) (m llvm-module) (v llvm-value) (variant i64)) llvm-value
  (if (if (eq variant 2) true (if (eq variant 8) true (eq variant 9)))
      (let ((args-ptr (alloca-args builder 1)))
        (store-arg builder args-ptr 0 v)
        (build-icmp-eq builder
                        (build-call builder (get-function m "rt_box_kind") args-ptr 1)
                        (const-i64 builder (if (eq variant 2) 1 (if (eq variant 8) 2 3)))))
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
                                               (if (eq variant 3) 4
                                                   (if (eq variant 5) 2
                                                       (if (eq variant 6) 3
                                                           (if (eq variant 7) 1
                                                               (panic "compile-sexpr-tag-test: unknown Sexpr variant")))))))))))))

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

;; The box-scrutinee counterpart of `compile-sexpr-tag-test`: an enum value
;; (`Option`/`Result`/a `defenum`, built by `compile-construct-box` via
;; `rt_data_new`) is a `BoxedObj::Enum` now — the enum-representation
;; unification's compiler flip retired the raw `malloc`'d array this used
;; to read directly, in favor of the `rt_data_variant` FFI call (mirroring
;; `compile-struct-field`'s own `rt_struct_field_get` call for a boxed
;; struct's field).
(defun compile-box-tag-test ((builder llvm-builder) (m llvm-module) (v llvm-value) (variant i64)) llvm-value
  (let ((args-ptr (alloca-args builder 1)))
    (store-arg builder args-ptr 0 v)
    (build-icmp-eq builder (build-call builder (get-function m "rt_data_variant") args-ptr 1) (const-i64 builder variant))))

;; The box-scrutinee counterpart of `compile-sexpr-field`/
;; `compile-struct-field`: field `idx` of a boxed enum value, fetched via
;; `rt_data_field` (the enum peer of `rt_struct_field_get`) then decoded
;; through `compile-sexpr-field` per this field's own `kind`
;; (`ast_bridge::struct_field_kind`'s numbering — enum fields are tagged the
;; same way a struct's are now, see `translate_construct`'s doc comment).
(defun compile-box-field ((builder llvm-builder) (m llvm-module) (v llvm-value) (kind i64) (idx i32)) llvm-value
  (let ((args-ptr (alloca-args builder 2)))
    (store-arg builder args-ptr 0 v)
    (store-arg builder args-ptr 1 (const-i64 builder (as i64 idx)))
    (let ((raw (build-call builder (get-function m "rt_data_field") args-ptr 2)))
      (compile-sexpr-field builder m raw kind 0))))

;; The struct-scrutinee counterpart of `compile-sexpr-field`/
;; `compile-box-field`: field `idx` of a boxed struct `v` (already a properly
;; tagged `Sexpr`, unlike a sum-ADT box's untagged raw pointer -- there is no
;; `build-int-to-ptr` here) is fetched via `rt_struct_field_get` -- the exact
;; same call `compile-field-get` makes -- then decoded through
;; `compile-sexpr-field` per this field's own `kind`
;; (`ast_bridge::struct_field_kind`'s numbering, reused verbatim; a scalar/
;; passthrough kind's own `idx` parameter is unused, so `0` is passed, the
;; same convention `compile-field-get` follows).
(defun compile-struct-field ((builder llvm-builder) (m llvm-module) (v llvm-value) (kind i64) (idx i32)) llvm-value
  (let ((args-ptr (alloca-args builder 2)))
    (store-arg builder args-ptr 0 v)
    (store-arg builder args-ptr 1 (const-i64 builder (as i64 idx)))
    (let ((raw (build-call builder (get-function m "rt_struct_field_get") args-ptr 2)))
      (compile-sexpr-field builder m raw kind 0))))

;; The literal tagged `Sexpr::Str` "option", built directly from raw
;; Unicode scalar constants (`rt_str_new`'s own "raw char scalars"
;; contract — no `ast_bridge::str_literal_form` AST node needed) — for a
;; natively-compiled builtin that must synthesize a real `Option<T>` value
;; with no corresponding source-level `Option::some`/`none` call site to
;; derive a type-name form from (`try-bignum->int`'s `rt_data_new` calls
;; below being the one case today).
(defun compile-option-type-name ((builder llvm-builder) (m llvm-module)) llvm-value
  (let ((args-ptr (alloca-args builder 6)))
    (store-arg builder args-ptr 0 (const-i64 builder 111))
    (store-arg builder args-ptr 1 (const-i64 builder 112))
    (store-arg builder args-ptr 2 (const-i64 builder 116))
    (store-arg builder args-ptr 3 (const-i64 builder 105))
    (store-arg builder args-ptr 4 (const-i64 builder 111))
    (store-arg builder args-ptr 5 (const-i64 builder 110))
    (build-call builder (get-function m "rt_str_new") args-ptr 6)))

(defun compile-function ((m llvm-module) (name string) (param-names Sexpr) (body Sexpr)) llvm-module
    (let ((f (add-function m name)))
      (let ((b (append-block f "entry")))
        (let ((builder (llvm-builder::create)))
          (position-at-end builder b)
          (let ((env (new-env)))
            (bind-params env builder m f param-names 0)
            (retain-bindings builder m env param-names)
            (let ((fn-env (new-fn-env)))
              (labels ((compile-value ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((s (sexpr-sym-name (sexpr-car e))))
                            (if (equal s "int")
                                (compile-int builder e)
                                (if (equal s "char")
                                    (compile-char builder e)
                                (if (equal s "bool")
                                    (compile-bool builder e)
                                    (if (equal s "float")
                                        (compile-float builder e)
                                    (if (equal s "str")
                                        (compile-str builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                    (if (equal s "bignum")
                                        (compile-bignum-literal builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                    (if (equal s "ratio")
                                        (compile-ratio-literal builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                    (if (equal s "unit")
                                        (compile-unit builder)
                                        (if (equal s "var")
                                            (compile-var builder env fn-env captured e)
                                            (if (equal s "cellvar")
                                            (compile-cellvar builder env fn-env captured e)
                                            (if (equal s "llvm-op")
                                                (compile-llvm-op builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                            (if (equal s "assoc")
                                                (compile-assoc builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                (if (equal s "apply")
                                                    (compile-apply builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                    (if (equal s "labels")
                                                        (compile-labels builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                        (if (equal s "call")
                                                            (compile-call builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                            (if (equal s "lambda")
                                                                (compile-lambda builder env fn-env captured e)
                                                                (if (equal s "apply-indirect")
                                                                    (compile-apply-indirect builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                    (if (equal s "if")
                                                                        (compile-if builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                        (if (equal s "let")
                                                                            (compile-let builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                            (if (equal s "loop")
                                                                                (compile-loop builder env fn-env captured cur-fn e)
                                                                                (if (equal s "break")
                                                                                    (compile-break builder loop-exit loop-slot loop-root-base)
                                                                                    (if (equal s "return")
                                                                                        (compile-return builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                        (if (equal s "set")
                                                                                            (compile-set builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                            (if (equal s "cellset")
                                                                                            (compile-cellset builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                            (if (equal s "match")
                                                                                                (compile-match builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                (if (equal s "construct")
                                                                                                    (compile-construct builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                    (if (equal s "field-get")
                                                                                                        (compile-field-get builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                        (if (equal s "field-set")
                                                                                                            (compile-field-set builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                            (if (equal s "global")
                                                                                                                (compile-global builder e)
                                                                                                                (if (equal s "set-global")
                                                                                                                    (compile-set-global builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                                    (if (equal s "global-init")
                                                                                                                        (compile-global-init builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                                        (if (equal s "panic")
                                                                                                                            (compile-panic builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                                            (if (equal s "vector-op")
                                                                                                                                (compile-vector-op builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                                                (if (equal s "hashtable-op")
                                                                                                                                    (compile-hashtable-op builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base e)
                                                                                                                                    (panic (append "compile-value: unsupported tag " s)))))))))))))))))))))))))))))))))))))
                           )
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
                         (const-i64 builder (sexpr-int (sexpr-car (sexpr-cdr e)))))
                       ;; `(char c)` -- a bare `char` literal. Compiled the same
                       ;; way `compile-int` is (a plain, untagged `i64` scalar --
                       ;; `char->int`'s own Unicode-scalar-value payload, widened
                       ;; from its `i32` result via the zero-cost `i32`<->`i64`
                       ;; `as` relabel `Checker::check_as` already allows), never
                       ;; a tagged `Sexpr::Char` (that's `compile-construct-
                       ;; sexpr`'s own variant-3 arm, which calls this exactly
                       ;; like `compile-int`'s own variant-1 arm calls
                       ;; `compile-int`). Added alongside `sexpr-char`
                       ;; (`registry.rs`/`Interp::eval_builtin`) -- a bare
                       ;; `char` literal previously had no dispatch tag at all
                       ;; (an oversight discovered while implementing
                       ;; `Expr::Quote`, whose `Char` leaf needs exactly this).
                       (compile-char ((builder llvm-builder) (e Sexpr)) llvm-value
                         (const-i64 builder (as i64 (char->int (sexpr-char (sexpr-car (sexpr-cdr e)))))))
                       ;; `(bool b)` (if/let/comparisons, labels/closures
                       ;; Stage 5) — every compiled value is a plain `i64`
                       ;; (see `registry::llvm_module_def`'s doc comment), so
                       ;; a `bool` literal is just `0`/`1`, the same
                       ;; representation `build-icmp-*` already produces and
                       ;; `compile-if`'s `build-cond-br` already expects.
                       (compile-bool ((builder llvm-builder) (e Sexpr)) llvm-value
                         (if (sexpr-bool (sexpr-car (sexpr-cdr e))) (const-i64 builder 1) (const-i64 builder 0)))
                       ;; `(float bits)` (Sexpr/RtValue unification, Stage 0)
                       ;; -- a bare `f64` literal. `bits` is the literal's raw
                       ;; `f64::to_bits` pattern embedded as a plain `Int`
                       ;; node by `ast_bridge::ast_to_sexpr_scoped`'s
                       ;; `Expr::Float` arm. Like `compile-int`, this returns
                       ;; the *plain, untagged* bit pattern -- **not** a boxed
                       ;; `Sexpr::Float` (that would be `rt_float_new`,
                       ;; wrongly called twice: once here, once more by
                       ;; `compile-construct-sexpr`'s own variant-2 arm on
                       ;; the tagged result this function already returned --
                       ;; boxing belongs to construct-sexpr alone, the exact
                       ;; same division of labor `compile-int`/variant-1
                       ;; already has: `compile-int` returns a bare `i64`,
                       ;; `compile-construct-sexpr`'s `build-shl`/tag-OR is
                       ;; what actually makes it a `Sexpr::Int`).
                       (compile-float ((builder llvm-builder) (e Sexpr)) llvm-value
                         (const-i64 builder (sexpr-int (sexpr-car (sexpr-cdr e)))))
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
                         (let ((chars (sexpr-cdr e)))
                           (let ((n (sexpr-list-length chars)))
                             (let ((args-ptr (alloca-args builder n)))
                               (store-str-chars builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr chars 0)
                               (build-call builder (get-function m "rt_str_new") args-ptr n)))))
                       ;; Fills a `compile-str`-allocated array, one compiled
                       ;; `(int c)` character per slot — the `str`-literal
                       ;; analogue of `compile-construct-box-fields`.
                       (store-str-chars ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (args-ptr llvm-value) (forms Sexpr) (idx i32)) ()
                         (if (sexpr-consp forms)
                             (let ((form (sexpr-car forms)) (rest (sexpr-cdr forms)))
                               (store-arg builder args-ptr idx (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form))
                               (store-str-chars builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr rest (+ idx 1)))
                             ()))
                       ;; `(bignum (int sign) (int d0) (int d1) ...)` —
                       ;; `ast_bridge::bignum_literal_form`'s doc comment.
                       ;; Same shape as `compile-str`/`store-str-chars` (one
                       ;; `(int _)` node per raw payload scalar, reused
                       ;; verbatim to fill the args array), calling
                       ;; `rt_bignum_new` instead of `rt_str_new`. Like `str`,
                       ;; the result is already a fully tagged `Sexpr::Bignum`
                       ;; — no further bit manipulation needed at the
                       ;; `compile-construct-sexpr`/`compile-sexpr-field`
                       ;; boundary (both treat variant `8` as passthrough).
                       (compile-bignum-literal ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((parts (sexpr-cdr e)))
                           (let ((n (sexpr-list-length parts)))
                             (let ((args-ptr (alloca-args builder n)))
                               (store-str-chars builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr parts 0)
                               (build-call builder (get-function m "rt_bignum_new") args-ptr n)))))
                       ;; `(ratio numer-form denom-form)` —
                       ;; `ast_bridge::ratio_literal_form`'s doc comment. Both
                       ;; sub-forms are themselves `(bignum ...)` nodes,
                       ;; compiled via the ordinary `compile-value` dispatch
                       ;; (so a `ratio` literal's numerator/denominator go
                       ;; through `compile-bignum-literal` exactly like any
                       ;; other bignum), then combined with
                       ;; `rt_ratio_from_bignums` — same GC-root discipline as
                       ;; `compile-construct-sexpr`'s `Cons` (variant `7`)
                       ;; case: each already-boxed sub-value is rooted before
                       ;; the next is built, in case building it triggers a
                       ;; collection.
                       (compile-ratio-literal ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((numer-form (sexpr-car (sexpr-cdr e))) (denom-form (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
                           (let ((args-ptr (alloca-args builder 2)))
                             (let ((numer-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base numer-form)))
                               (store-arg builder args-ptr 0 numer-v)
                               (push-sexpr-root builder m numer-v)
                               (let ((denom-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base denom-form)))
                                 (store-arg builder args-ptr 1 denom-v)
                                 (push-sexpr-root builder m denom-v)
                                 (let ((result (build-call builder (get-function m "rt_ratio_from_bignums") args-ptr 2)))
                                   (pop-sexpr-root builder m)
                                   (pop-sexpr-root builder m)
                                   result))))))
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
                         (resolve-value builder env fn-env captured (sexpr-str (sexpr-car (sexpr-cdr e)))))
                       ;; `(cellvar name kind)` — a reference to a cell-boxed
                       ;; name (`ast_bridge`'s `cx.cell_names`, closure-
                       ;; representation unification Stage 4): unlike
                       ;; `compile-var`, `env`'s slot holds a *cell reference*
                       ;; (`bind-params`/`bind-let-values`/`bind-captures`'
                       ;; own `kind >= 10` branches), not the value itself, so
                       ;; this dereferences it via `rt_cell_get` and detags the
                       ;; result per `kind` (`compile-sexpr-field`, the exact
                       ;; same decoder a `defstruct` field's own read uses).
                       ;; Always resolves through `env` — a `labels`
                       ;; sibling/self name is never cell-boxed (siblings
                       ;; always go through `fn-env`/direct calls, never a
                       ;; captured binding), so unlike `resolve-value` there is
                       ;; no `fn-env` fallback to try; a name absent from
                       ;; `env` here is a genuine internal-invariant break
                       ;; (`ast_bridge` only ever emits this tag for a name it
                       ;; already knows is a cell-boxed local/param/capture).
                       (compile-cellvar ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (e Sexpr)) llvm-value
                         (let ((name (sexpr-str (sexpr-car (sexpr-cdr e)))))
                           (let ((kind (sexpr-int (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
                             (match (get env name)
                               ((Some slot)
                                (let ((cell-ref (load-raw builder slot 0)))
                                  (let ((args-ptr (alloca-args builder 1)))
                                    (store-arg builder args-ptr 0 cell-ref)
                                    (let ((raw (build-call builder (get-function m "rt_cell_get") args-ptr 1)))
                                      (compile-sexpr-field builder m raw kind 0)))))
                               (None (panic (append "compile-cellvar: unbound variable " name)))))))
                       ;; Resolves `name` to a value two ways, in order:
                       ;; (1) an ordinary binding in `env` (a parameter, or a
                       ;; captured name already loaded by `bind-captures`) —
                       ;; unchanged by this lookup; (2) a currently in-scope
                       ;; `labels` sibling/self, found in `fn-env` instead —
                       ;; boxed into a *fresh* `BoxedObj::CompiledClosure` on
                       ;; the spot (`build-make-closure`, exactly the way
                       ;; `compile-lambda` already boxes a `lambda` literal),
                       ;; using *this* scope's own shared `captured` list to
                       ;; build the box's env array. That array is built via
                       ;; `compile-escaping-env-args`, not the lighter
                       ;; `compile-env-args` a direct call uses, purely
                       ;; because this fresh box may itself escape (e.g. it's
                       ;; what a `labels` block's trailing body bare-returns)
                       ;; while a direct call's own env array never does —
                       ;; both fill their array identically now that neither
                       ;; needs any retain/release bookkeeping.
                       ;; `compile-escaping-env-args` is mutually
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
                       ;; sibling name. Two boxes built this way *can*
                       ;; reference each other (a `labels` block with two
                       ;; siblings that each capture the other's boxed value
                       ;; as data forms a genuine reference cycle) — unlike
                       ;; the retired `ClosureBox` refcount scheme, this is
                       ;; unproblematic now: `BoxedObj::CompiledClosure` is an
                       ;; ordinary GC-heap value, and mark-sweep reclaims a
                       ;; cycle just as readily as any other unreachable
                       ;; structure once nothing external reaches it.
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
                                        (build-make-closure builder m target env-ptr env-len (compute-sexpr-mask captured 0)))))
                                   (None (panic (append "compile-var: unbound variable " name)))))))
                       ;; Fills `env-ptr` for a *direct* (non-escaping)
                       ;; call's env array — `compile-apply`'s sibling-to-
                       ;; sibling calls. No retain/release bookkeeping is
                       ;; needed here at all now that a compiled closure is
                       ;; an ordinary GC-heap value (the closure-
                       ;; representation unification retired the
                       ;; `ClosureBox` refcount scheme this used to feed a
                       ;; `pending-ptr` post-call-release array for — see
                       ;; `resolve-value`'s doc comment for why a *fresh*
                       ;; value boxed on demand here needs no release of its
                       ;; own either): a value just flows into the env array
                       ;; unchanged. `names`/`idx` are what this call is
                       ;; actually walking to fill `env-ptr`; `captured` is
                       ;; held constant across every recursive step — the
                       ;; *enclosing* scope's own shared captured-name list,
                       ;; passed through unchanged purely so `resolve-value`'s
                       ;; fallback has it on hand if any name along the way
                       ;; turns out to be a sibling rather than an ordinary
                       ;; value. At every existing call site, `names` and
                       ;; `captured` happen to be the *same* list (building a
                       ;; block's own shared env array).
                       (compile-env-args ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (env-ptr llvm-value) (names Sexpr) (idx i32)) ()
                         (if (sexpr-consp names)
                             (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
                              (let ((nm (sexpr-sym-name (sexpr-car name-pair))))
                                (let ((v (resolve-value builder env fn-env captured nm)))
                                  (store-arg builder env-ptr idx v)
                                  (compile-env-args builder env fn-env captured env-ptr rest (+ idx 1)))))
                             ()))
                       ;; Fills `env-ptr` for an *escaping* `ClosureBox`'s
                       ;; own captured-value array — `compile-lambda`'s and
                       ;; `resolve-value`'s fallback's use. No retain is
                       ;; needed here either, for the same reason
                       ;; `compile-env-args` no longer needs one: the box this
                       ;; array feeds is now a GC-traced `BoxedObj::
                       ;; CompiledClosure` (`rt_closure_new`), reachable from
                       ;; wherever the box itself is reachable — a captured
                       ;; value just moves into its env slot with no
                       ;; ownership bookkeeping of its own.
                       (compile-escaping-env-args ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (env-ptr llvm-value) (names Sexpr) (idx i32)) ()
                         (if (sexpr-consp names)
                             (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
                              (let ((nm (sexpr-sym-name (sexpr-car name-pair))))
                                (let ((v (resolve-value builder env fn-env captured nm)))
                                  (store-arg builder env-ptr idx v)
                                  (compile-escaping-env-args builder env fn-env captured env-ptr rest (+ idx 1)))))
                             ()))
                       ;; `(assoc type-name method instance arg...)` — two
                       ;; cases, dispatched *per (type-name, method) pair*,
                       ;; not per receiver type alone. `i64`/`i32` receivers
                       ;; (the same runtime representation, `RtValue::Int(i64)`,
                       ;; so one code path covers both) whose method is on
                       ;; `int-native-method?`'s list compile straight to the
                       ;; matching LLVM instruction, exactly as before; this
                       ;; guard still matters because `f64` defines methods
                       ;; under the same names (`registry::float_assoc`), so
                       ;; without it `(< 1.0 2.0)` would quietly compile as
                       ;; an integer comparison. Everything else — any other
                       ;; `type-name`, and also a *user-defined* method on a
                       ;; primitive receiver (the prelude's `impl Eq i32` →
                       ;; `i32::equals`) — is a call to a user-defined method
                       ;; (a `defmethod`/`defstruct` accessor or setter),
                       ;; handled by `compile-assoc-user` below. The
                       ;; native-method predicates must be consulted *before*
                       ;; any argument is compiled: the native chains
                       ;; pre-compile their operands as raw values, so a
                       ;; fallback placed at the end of a chain would emit
                       ;; the argument IR twice (double-evaluating any
                       ;; side effects).
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
                       ;; (no compiled-code primitive backs them yet); a
                       ;; non-native string method falls through to
                       ;; `compile-assoc-user` like any other user method,
                       ;; and `get-function` panics clearly there if it was
                       ;; never compiled.
                       (compile-assoc ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((type-name (sexpr-str (sexpr-car (sexpr-cdr e)))))
                           (let ((method (sexpr-str (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
                             (let ((rest (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                               (if (if (equal type-name "string") (string-native-method? method) false)
                                   (let ((a (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-cdr (sexpr-car rest)))))
                                     (if (equal method "length")
                                         (let ((args-ptr (alloca-args builder 1)))
                                           (store-arg builder args-ptr 0 a)
                                           (build-call builder (get-function m "rt_str_length") args-ptr 1))
                                         (let ((b (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
                                           (if (equal method "ref")
                                               (let ((args-ptr (alloca-args builder 2)))
                                                 (store-arg builder args-ptr 0 a)
                                                 (store-arg builder args-ptr 1 b)
                                                 (build-call builder (get-function m "rt_str_ref") args-ptr 2))
                                               (if (if (equal method "eq") true (equal method "equal"))
                                                   ;; `eq` and `equal` share `rt_str_eq` (content
                                                   ;; comparison) — the compiled story predates the
                                                   ;; eq/eql/equal redesign's Rc-identity `eq`, and
                                                   ;; `equal` (the prelude's `impl Eq string` body)
                                                   ;; is the content comparison it implements.
                                                   (let ((args-ptr (alloca-args builder 2)))
                                                     (store-arg builder args-ptr 0 a)
                                                     (store-arg builder args-ptr 1 b)
                                                     (build-call builder (get-function m "rt_str_eq") args-ptr 2))
                                                   ;; `equalp`: ASCII case-insensitive content equality
                                                   ;; (`rt_str_equalp`) — no in-place lowering, unlike
                                                   ;; `eq`/`equal`'s plain content compare above.
                                                   (if (equal method "equalp")
                                                       (let ((args-ptr (alloca-args builder 2)))
                                                         (store-arg builder args-ptr 0 a)
                                                         (store-arg builder args-ptr 1 b)
                                                         (build-call builder (get-function m "rt_str_equalp") args-ptr 2))
                                                   ;; `<`/`>`/`<=`/`>=` all derive from `rt_str_lt`
                                                   ;; (`str-lt-call`); `not` is `(icmp-eq v 0)`.
                                                   (if (if (equal method "lt") true (equal method "<"))
                                                       (str-lt-call builder m a b)
                                                       (if (equal method ">")
                                                           (str-lt-call builder m b a)
                                                           (if (equal method "<=")
                                                               (build-icmp-eq builder (str-lt-call builder m b a) (const-i64 builder 0))
                                                               (if (equal method ">=")
                                                                   (build-icmp-eq builder (str-lt-call builder m a b) (const-i64 builder 0))
                                                                   (if (equal method "append")
                                                                       (let ((args-ptr (alloca-args builder 2)))
                                                                         (store-arg builder args-ptr 0 a)
                                                                         (store-arg builder args-ptr 1 b)
                                                                         (build-call builder (get-function m "rt_str_append") args-ptr 2))
                                                                       (panic (append "compile-assoc: unsupported str method " method)))))))))))))
                                   (if (if (if (equal type-name "i64") true (equal type-name "i32")) (int-native-method? method) false)
                                       (let ((a (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-cdr (sexpr-car rest)))))
                                         ;; `int->bignum`/`int->ratio`: unary, checked before `b2`
                                         ;; is read — same reason `float-native-method?`'s own
                                         ;; unary conversions are checked first (there is no
                                         ;; second argument form to compile for these).
                                         (if (equal method "int->bignum")
                                             (let ((args-ptr (alloca-args builder 1)))
                                               (store-arg builder args-ptr 0 a)
                                               (build-call builder (get-function m "rt_int_to_bignum") args-ptr 1))
                                         (if (equal method "int->ratio")
                                             (let ((args-ptr (alloca-args builder 1)))
                                               (store-arg builder args-ptr 0 a)
                                               (build-call builder (get-function m "rt_int_to_ratio") args-ptr 1))
                                         (let ((b2 (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
                                           (if (equal method "+")
                                               (build-add builder a b2)
                                               (if (equal method "-")
                                                   (build-sub builder a b2)
                                                   (if (equal method "*")
                                                       (build-mul builder a b2)
                                                       (if (equal method "<")
                                                           (build-icmp-lt builder a b2)
                                                           (if (equal method "<=")
                                                               (build-icmp-le builder a b2)
                                                               (if (equal method ">")
                                                                   (build-icmp-gt builder a b2)
                                                                   (if (equal method ">=")
                                                                       (build-icmp-ge builder a b2)
                                                                       (if (if (equal method "=") true (equal method "eq"))
                                                                           (build-icmp-eq builder a b2)
                                                                           (if (equal method "/=")
                                                                               (build-icmp-ne builder a b2)
                                                                               (panic (append "compile-assoc: unsupported method " method)))))))))))))))
                                       (if (if (equal type-name "char") (char-native-method? method) false)
                                           ;; `char` receivers: raw `i64` code points in
                                           ;; compiled code, so the comparisons lower to the
                                           ;; same integer `icmp`s the int branch uses —
                                           ;; `lt`/`<`/`<=`/`>`/`>=` and `eq`/`eql`/`equal` → eq.
                                           ;; `equalp` alone folds case, so it calls the
                                           ;; `rt_char_equalp` runtime helper (raw code-point
                                           ;; args, matching this branch's own operands).
                                           (let ((a (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-cdr (sexpr-car rest)))))
                                             ;; `char->int` is unary (receiver only) and the
                                             ;; identity at the compiled level — return the
                                             ;; receiver's raw code point unchanged, before
                                             ;; the binary branch below tries to read a
                                             ;; (non-existent) second operand.
                                             (if (equal method "char->int")
                                                 a
                                             (let ((b2 (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
                                               (if (equal method "equalp")
                                                   (let ((args-ptr (alloca-args builder 2)))
                                                     (store-arg builder args-ptr 0 a)
                                                     (store-arg builder args-ptr 1 b2)
                                                     (build-call builder (get-function m "rt_char_equalp") args-ptr 2))
                                               (if (if (equal method "lt") true (equal method "<"))
                                                   (build-icmp-lt builder a b2)
                                                   (if (equal method "<=")
                                                       (build-icmp-le builder a b2)
                                                       (if (equal method ">")
                                                           (build-icmp-gt builder a b2)
                                                           (if (equal method ">=")
                                                               (build-icmp-ge builder a b2)
                                                               (build-icmp-eq builder a b2)))))))))
                                           ;; `f64` receivers: a compiled `f64` is
                                           ;; its raw bits in an `i64`, so arithmetic
                                           ;; lowers to `build-fadd`/... (each
                                           ;; bitcasts to `double` and back) and
                                           ;; comparisons to `build-fcmp-*`. `=` and
                                           ;; its `eq`/`eql`/`equal`/`equalp` aliases
                                           ;; all fold to the ordered `fcmp-eq`; `/=`
                                           ;; to `fcmp-ne` (unordered, matching Rust
                                           ;; `!=`); the arithmetic ops and `expt`
                                           ;; (`build-fpow`, LLVM's `llvm.pow.f64`
                                           ;; intrinsic) need a second operand `b2`;
                                           ;; the unary transcendental/rounding family
                                           ;; (`sqrt`/`floor`/`ceiling`/`round`/
                                           ;; `truncate`, each its own LLVM intrinsic
                                           ;; via `build-f*`), `float->int`
                                           ;; (`build-fptosi`, the saturating
                                           ;; `llvm.fptosi.sat` intrinsic — matches
                                           ;; the interpreter's `as`-cast semantics
                                           ;; on NaN/out-of-range input), and `float->bignum`/
                                           ;; `float->ratio` (`rt_float_to_bignum`/
                                           ;; `rt_float_to_ratio`, allocating heap
                                           ;; calls — the one exception in this group
                                           ;; that isn't a bare LLVM instruction) only
                                           ;; need `a`, so they're checked first to
                                           ;; avoid evaluating a nonexistent second
                                           ;; argument form.
                                           (if (if (equal type-name "f64") (float-native-method? method) false)
                                               (let ((a (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-cdr (sexpr-car rest)))))
                                                 (if (equal method "sqrt")
                                                     (build-fsqrt builder m a)
                                                     (if (equal method "floor")
                                                         (build-ffloor builder m a)
                                                         (if (equal method "ceiling")
                                                             (build-fceil builder m a)
                                                             (if (equal method "round")
                                                                 (build-fround builder m a)
                                                                 (if (equal method "truncate")
                                                                     (build-ftrunc builder m a)
                                                                     (if (equal method "float->int")
                                                                         (build-fptosi builder m a)
                                                                         (if (equal method "float->bignum")
                                                                             (let ((args-ptr (alloca-args builder 1)))
                                                                               (store-arg builder args-ptr 0 a)
                                                                               (build-call builder (get-function m "rt_float_to_bignum") args-ptr 1))
                                                                         (if (equal method "float->ratio")
                                                                             (let ((args-ptr (alloca-args builder 1)))
                                                                               (store-arg builder args-ptr 0 a)
                                                                               (build-call builder (get-function m "rt_float_to_ratio") args-ptr 1))
                                                                         (let ((b2 (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
                                                                           (if (equal method "+")
                                                                               (build-fadd builder a b2)
                                                                               (if (equal method "-")
                                                                                   (build-fsub builder a b2)
                                                                                   (if (equal method "*")
                                                                                       (build-fmul builder a b2)
                                                                                       (if (equal method "/")
                                                                                           (build-fdiv builder a b2)
                                                                                           (if (equal method "mod")
                                                                                               (build-frem builder a b2)
                                                                                               (if (equal method "expt")
                                                                                                   (build-fpow builder m a b2)
                                                                                                   (if (equal method "<")
                                                                                                       (build-fcmp-lt builder a b2)
                                                                                                       (if (equal method "<=")
                                                                                                           (build-fcmp-le builder a b2)
                                                                                                           (if (equal method ">")
                                                                                                               (build-fcmp-gt builder a b2)
                                                                                                               (if (equal method ">=")
                                                                                                                   (build-fcmp-ge builder a b2)
                                                                                                                   (if (equal method "/=")
                                                                                                                       (build-fcmp-ne builder a b2)
                                                                                                                       (build-fcmp-eq builder a b2))))))))))))))))))))))
                                               (if (if (equal type-name "bignum") (bignum-native-method? method) false)
    (let ((a (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-cdr (sexpr-car rest)))))
      (if (equal method "bignum->int")
          (bignum-unary-call builder m "rt_bignum_to_int" a)
      (if (equal method "bignum->float")
          (bignum-unary-call builder m "rt_bignum_to_float" a)
      (if (equal method "bignum->ratio")
          (bignum-unary-call builder m "rt_bignum_to_ratio" a)
      (if (equal method "try-bignum->int")
          ;; `Option<i32>` result: real control flow (found/overflow), the
          ;; same `compile-if`-shaped "alloca a merge slot, branch, store
          ;; each arm's result, load after the merge block" `compile-hashtable-op`'s
          ;; own `get`/`remove` case already uses — `rt_bignum_fits_i32`
          ;; checked first, `rt_bignum_to_int_raw` only called once that
          ;; confirms `1`. `Some`/`None` build a real `BoxedObj::Enum` via
          ;; `rt_data_new` now (the enum-representation unification's
          ;; compiler flip) — `compile-option-type-name` supplies the type
          ;; name (no source-level `Option::some`/`none` call site exists
          ;; here to derive one from) and the `i32` field is tagged via
          ;; `compile-tag-struct-field` (kind `1`) first, `rt_data_new`'s
          ;; contract being the same tagged-field one `rt_struct_new` has.
          (let ((fits-args (alloca-args builder 1)))
            (store-arg builder fits-args 0 a)
            (let ((fits (build-call builder (get-function m "rt_bignum_fits_i32") fits-args 1)))
              (let ((then-block (append-block cur-fn "bignum-fits")))
                (let ((else-block (append-block cur-fn "bignum-overflow")))
                  (let ((merge-block (append-block cur-fn "bignum-try-merge")))
                    (let ((slot (alloca-args builder 1)))
                      (build-cond-br builder fits then-block else-block)
                      (position-at-end builder then-block)
                      (let ((raw (build-call builder (get-function m "rt_bignum_to_int_raw") fits-args 1)))
                        (let ((some-args (alloca-args builder 3)))
                          (store-arg builder some-args 0 (compile-option-type-name builder m))
                          (store-arg builder some-args 1 (const-i64 builder 0))
                          (store-arg builder some-args 2 (compile-tag-struct-field builder m raw 1))
                          (let ((some-box (build-call builder (get-function m "rt_data_new") some-args 3)))
                            (push-permanent-sexpr-root builder m some-box)
                            (let ((ignored (store-arg builder slot 0 some-box)))
                              (build-br builder merge-block)))))
                      (position-at-end builder else-block)
                      (let ((none-args (alloca-args builder 2)))
                        (store-arg builder none-args 0 (compile-option-type-name builder m))
                        (store-arg builder none-args 1 (const-i64 builder 1))
                        (let ((none-box (build-call builder (get-function m "rt_data_new") none-args 2)))
                          (push-permanent-sexpr-root builder m none-box)
                          (let ((ignored (store-arg builder slot 0 none-box)))
                            (build-br builder merge-block))))
                      (position-at-end builder merge-block)
                      (load-raw builder slot 0)))))))
          (let ((b2 (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
            (if (equal method "+")
                (bignum-binop-call builder m "rt_bignum_add" a b2)
                (if (equal method "-")
                    (bignum-binop-call builder m "rt_bignum_sub" a b2)
                    (if (equal method "*")
                        (bignum-binop-call builder m "rt_bignum_mul" a b2)
                        (if (equal method "/")
                            (bignum-binop-call builder m "rt_bignum_div" a b2)
                            (if (equal method "mod")
                                (bignum-binop-call builder m "rt_bignum_mod" a b2)
                                (if (equal method "<")
                                    (build-icmp-lt builder (bignum-cmp-call builder m a b2) (const-i64 builder 0))
                                    (if (equal method "<=")
                                        (build-icmp-le builder (bignum-cmp-call builder m a b2) (const-i64 builder 0))
                                        (if (equal method ">")
                                            (build-icmp-gt builder (bignum-cmp-call builder m a b2) (const-i64 builder 0))
                                            (if (equal method ">=")
                                                (build-icmp-ge builder (bignum-cmp-call builder m a b2) (const-i64 builder 0))
                                                (if (equal method "/=")
                                                    (build-icmp-ne builder (bignum-cmp-call builder m a b2) (const-i64 builder 0))
                                                    (build-icmp-eq builder (bignum-cmp-call builder m a b2) (const-i64 builder 0))))))))))))))))))
    (if (if (equal type-name "ratio") (ratio-native-method? method) false)
        (let ((a (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-cdr (sexpr-car rest)))))
          (if (equal method "ratio->bignum")
              (ratio-unary-call builder m "rt_ratio_to_bignum" a)
          (if (equal method "ratio->float")
              (ratio-unary-call builder m "rt_ratio_to_float" a)
          (if (equal method "numerator")
              (ratio-unary-call builder m "rt_ratio_numerator" a)
          (if (equal method "denominator")
              (ratio-unary-call builder m "rt_ratio_denominator" a)
          (let ((b2 (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
            (if (equal method "+")
                (ratio-binop-call builder m "rt_ratio_add" a b2)
                (if (equal method "-")
                    (ratio-binop-call builder m "rt_ratio_sub" a b2)
                    (if (equal method "*")
                        (ratio-binop-call builder m "rt_ratio_mul" a b2)
                        (if (equal method "/")
                            (ratio-binop-call builder m "rt_ratio_div" a b2)
                            (if (equal method "<")
                                (build-icmp-lt builder (ratio-cmp-call builder m a b2) (const-i64 builder 0))
                                (if (equal method "<=")
                                    (build-icmp-le builder (ratio-cmp-call builder m a b2) (const-i64 builder 0))
                                    (if (equal method ">")
                                        (build-icmp-gt builder (ratio-cmp-call builder m a b2) (const-i64 builder 0))
                                        (if (equal method ">=")
                                            (build-icmp-ge builder (ratio-cmp-call builder m a b2) (const-i64 builder 0))
                                            (if (equal method "/=")
                                                (build-icmp-ne builder (ratio-cmp-call builder m a b2) (const-i64 builder 0))
                                                (build-icmp-eq builder (ratio-cmp-call builder m a b2) (const-i64 builder 0)))))))))))))))))
        (compile-assoc-user builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base type-name method rest)))))))))))
                       ;; The user-defined-method leg of `compile-assoc`'s
                       ;; dispatch (see its doc comment): call the callee
                       ;; under the mangled name `type-name::method`,
                       ;; arguments through `compile-call-args` exactly like
                       ;; an ordinary call's. A `labels` sibling (not a
                       ;; toplevel `defun`) because it closes over `m` and
                       ;; mutually recurses with `compile-call-args`.
                       (compile-assoc-user ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (type-name string) (method string) (rest Sexpr)) llvm-value
                         (let ((mangled (append "tl_" (append type-name (append "::" method)))))
                           (let ((argc (sexpr-list-length rest)))
                             (let ((args-ptr (alloca-args builder argc)))
                               (let ((sexpr-roots (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr rest 0)))
                                 (let ((result (build-call builder (get-function m mangled) args-ptr argc)))
                                   (pop-sexpr-roots builder m sexpr-roots)
                                   result))))))
                       ;; `(llvm-op opid (kind . arg)...)` — an `llvm-*`/
                       ;; native-`Scope<V>` builtin method call
                       ;; (`ast_bridge`'s `Expr::Assoc` lowering, interp-
                       ;; closure removal Stage 1): one call to the generic
                       ;; Rust-side dispatch shim `rt_llvm_call`, passing the
                       ;; translate-time-resolved op id (`ast_bridge::
                       ;; llvm_op_id`'s stable hash) in slot 0 and the
                       ;; compiled arguments after it. Argument handling is
                       ;; exactly `compile-assoc-user`'s (`compile-call-args`
                       ;; roots the tagged-`Sexpr`-kind ones across later
                       ;; arguments' own allocations); the op id itself is a
                       ;; raw constant with nothing to root. The result's
                       ;; encoding (handle / unit / bool / tagged str /
                       ;; boxed `Option`) is dictated by the node's checked
                       ;; type, the same as every other compiled value.
                       (compile-llvm-op ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((opid (sexpr-int (sexpr-car (sexpr-cdr e)))))
                           (let ((arg-forms (sexpr-cdr (sexpr-cdr e))))
                             (let ((argc (+ (sexpr-list-length arg-forms) 1)))
                               (let ((args-ptr (alloca-args builder argc)))
                                 (store-arg builder args-ptr 0 (const-i64 builder opid))
                                 (let ((sexpr-roots (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr arg-forms 1)))
                                   (let ((result (build-call builder (get-function m "rt_llvm_call") args-ptr argc)))
                                     (pop-sexpr-roots builder m sexpr-roots)
                                     result)))))))
                       ;; Fills a previously-`alloca-args`'d array, one
                       ;; compiled argument per slot, exactly as before —
                       ;; each `forms` element is a `(kind . arg-form)` pair
                       ;; (`ast_bridge::tagged_ast_list_to_sexpr`), but no
                       ;; retain/release bookkeeping is keyed on `kind`
                       ;; anymore (the closure-representation unification
                       ;; retired the `ClosureBox` refcount scheme a `kind =
                       ;; 1` argument used to need a post-call release for —
                       ;; see `compile-env-args`'s doc comment). A `kind = 2`
                       ;; (`Sexpr`, which now includes every `Fn`-typed
                       ;; argument too) gets `push-sexpr-root`ed right here,
                       ;; *unconditionally*: this slot's value would
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
                       (compile-call-args ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (args-ptr llvm-value) (forms Sexpr) (idx i32)) i32
                         (if (sexpr-consp forms)
                             (let ((arg-pair (sexpr-car forms)) (rest (sexpr-cdr forms)))
                              (let ((kind (sexpr-int (sexpr-car arg-pair))))
                              (let ((form (sexpr-cdr arg-pair)))
                                (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form)))
                                  (store-arg builder args-ptr idx v)
                                  (if (eq kind 2)
                                      (let ((ignored (push-sexpr-root builder m v)))
                                        (+ 1 (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr rest (+ idx 1))))
                                      (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr rest (+ idx 1)))))))
                             0))
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
                       ;; `nm` actually names.
                       (compile-apply ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((nm (sexpr-str (sexpr-car (sexpr-cdr e)))))
                           (let ((arg-forms (sexpr-cdr (sexpr-cdr e))))
                             (let ((argc (sexpr-list-length arg-forms)))
                               (let ((args-ptr (alloca-args builder argc)))
                                 (let ((sexpr-roots (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr arg-forms 0)))
                                   (match (get fn-env nm)
                                     ((Some target)
                                      (let ((env-len (sexpr-list-length captured)))
                                        (if (eq env-len 0)
                                            (let ((result (build-call builder target args-ptr argc)))
                                              (pop-sexpr-roots builder m sexpr-roots)
                                              result)
                                            (let ((env-ptr (alloca-args builder env-len)))
                                              (compile-env-args builder env fn-env captured env-ptr captured 0)
                                              (let ((result (build-call-with-env builder target args-ptr argc env-ptr env-len)))
                                                (pop-sexpr-roots builder m sexpr-roots)
                                                result)))))
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
                       ;; array: always a plain `build-call`.
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
                         (let ((raw-nm (sexpr-str (sexpr-car (sexpr-cdr e)))))
                           ;; The `sexpr-*` island layer maps to its
                           ;; `crate::compile::runtime` cons-heap shims (Symbol/
                           ;; Sexpr redesign Phase 4b — the free `car`/`cdr`/
                           ;; `cons` names are the `cons<T,U>` pair now, compiled
                           ;; as ordinary methods/`defun`s, not `rt_*` shims).
                           (let ((nm (if (equal raw-nm "sexpr-car") "rt_car"
                                         (if (equal raw-nm "sexpr-cdr") "rt_cdr"
                                             (if (equal raw-nm "sexpr-cons") "rt_cons"
                                                 (if (equal raw-nm "sexpr-consp") "rt_consp"
                                                     (if (equal raw-nm "sexpr-null") "rt_null"
                                                         (if (equal raw-nm "sexpr-atom") "rt_atom"
                                                             (if (equal raw-nm "sexpr-symp") "rt_symp"
                                                                 (if (equal raw-nm "sexpr-int") "rt_sexpr_int"
                                                                     (if (equal raw-nm "sexpr-bool") "rt_sexpr_bool"
                                                                         (if (equal raw-nm "sexpr-char") "rt_sexpr_char"
                                                                             (if (equal raw-nm "sexpr-float") "rt_float_value"
                                                                                 (if (equal raw-nm "sexpr-str") "rt_sexpr_str"
                                                                                     (if (equal raw-nm "sexpr-sym-name") "rt_sym_name"
                                                                                         raw-nm)))))))))))))))
                             (let ((arg-forms (sexpr-cdr (sexpr-cdr e))))
                               (let ((argc (sexpr-list-length arg-forms)))
                                 (let ((args-ptr (alloca-args builder argc)))
                                   (let ((sexpr-roots (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr arg-forms 0)))
                                     (let ((result (build-call builder (get-function m nm) args-ptr argc)))
                                       (pop-sexpr-roots builder m sexpr-roots)
                                       result))))))))
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
                       ;; get a `BoxedObj::CompiledClosure` reference, then
                       ;; called through `build-closure-apply` rather than
                       ;; `build-call`: nothing here can know ahead of time
                       ;; which compiled function it'll actually be. No
                       ;; release follows the call now (the closure-
                       ;; representation unification retired the `ClosureBox`
                       ;; refcount scheme that used to need one for a *fresh*
                       ;; callee, e.g. a bare-referenced `labels` sibling
                       ;; boxed on the spot by `resolve-value` — an
                       ;; unreferenced one is simply left for the GC now,
                       ;; same as any other unreferenced heap value).
                       (compile-apply-indirect ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((callee-form (sexpr-car (sexpr-cdr e))))
                           (let ((closure (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base callee-form)))
                             (let ((arg-forms (sexpr-cdr (sexpr-cdr e))))
                               (let ((argc (sexpr-list-length arg-forms)))
                                 (let ((args-ptr (alloca-args builder argc)))
                                   (let ((sexpr-roots (compile-call-args builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr arg-forms 0)))
                                     (let ((result (build-closure-apply builder m closure args-ptr argc)))
                                       (pop-sexpr-roots builder m sexpr-roots)
                                       result))))))))
                       ;; `compile-if`'s one helper (if/let/comparisons,
                       ;; labels/closures Stage 5): compiles `form` (one of
                       ;; an `if`'s `then`/`else` branches) and hands the
                       ;; result straight back — no retain step: unlike the
                       ;; old `ClosureBox` refcount scheme (where a *borrowed*
                       ;; `Fn`-typed branch value needed its own retain before
                       ;; crossing the `if`'s merge slot, since that boundary
                       ;; introduces no function activation to freshen it at),
                       ;; a GC-heap value has no ownership to freshen — it
                       ;; just flows through. `is-fn` is accordingly unused
                       ;; now, kept only so `compile-if`/`compile-return`/
                       ;; `compile-set`/`compile-match-arms` don't need their
                       ;; own separate call shape.
                       (compile-if-branch ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (is-fn bool) (form Sexpr)) llvm-value
                         (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form))
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
                         (let ((is-fn (sexpr-bool (sexpr-car (sexpr-cdr e)))))
                           (let ((cond-form (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
                             (let ((then-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                               (let ((else-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))
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
                       (compile-let-values ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (bindings Sexpr) (acc Scope<llvm-value>)) ()
                         (if (sexpr-consp bindings)
                             (let ((pair (sexpr-car bindings)) (rest (sexpr-cdr bindings)))
                              (let ((nm (sexpr-sym-name (sexpr-car (sexpr-car pair)))))
                              (let ((form (sexpr-cdr pair)))
                                (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form)))
                                  (set acc nm v)
                                  (compile-let-values builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base rest acc)))))
                             ()))
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
                         (if (sexpr-consp forms)
                             (let ((form (sexpr-car forms)) (rest (sexpr-cdr forms)))
                              (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form)))
                              (if (block-terminated? builder)
                                  v
                                  (if (sexpr-consp rest)
                                      (compile-let-body builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base rest)
                                      v))))
                             (compile-unit builder)))
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
                         (let ((bindings (sexpr-car (sexpr-cdr e))))
                           (let ((body-forms (sexpr-cdr (sexpr-cdr e))))
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
                       ;; doc comment), R1-pushing GC roots for its own
                       ;; params/captures on entry and R2-popping them on
                       ;; exit exactly like `compile-function`'s own
                       ;; top-level body and each `labels` sibling's body do,
                       ;; then builds an env array from `lcaptured`'s
                       ;; *current* values in the *outer* `env`
                       ;; (`compile-escaping-env-args`) and wraps the whole
                       ;; thing into a `BoxedObj::CompiledClosure`
                       ;; (`build-make-closure`, with `compute-sexpr-mask`'s
                       ;; bitmask so the GC mark phase knows which of its own
                       ;; captured slots are tagged values to trace) — the
                       ;; value this whole node evaluates to.
                       (compile-lambda ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (e Sexpr)) llvm-value
                         (let ((lname (sexpr-str (sexpr-car (sexpr-cdr e)))))
                           (let ((lcaptured (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
                             (let ((lparams (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                               (let ((lbody (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))
                                 (let ((nested-fn (add-function-with-env m lname)))
                                   (let ((nested-block (append-block nested-fn "entry")))
                                     (let ((nested-builder (llvm-builder::create)))
                                       (position-at-end nested-builder nested-block)
                                       (let ((nested-env (new-env)))
                                         (bind-params nested-env nested-builder m nested-fn lparams 0)
                                         (retain-bindings nested-builder m nested-env lparams)
                                         ;; No `retain-bindings` call for
                                         ;; `lcaptured` here (unlike
                                         ;; `lparams` just above): every
                                         ;; entry a captured-name list can
                                         ;; ever hold is `kind >= 10` now
                                         ;; (Stage 4), and `bind-captures`
                                         ;; itself already pushes a
                                         ;; permanent root for each one — a
                                         ;; `retain-bindings` pass here would
                                         ;; only ever match its now-unused
                                         ;; `kind = 2` case, a pure no-op.
                                         (bind-captures nested-env nested-builder m nested-fn lcaptured 0)
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
                                           (release-bindings nested-builder m nested-env lparams)
                                           (release-bindings nested-builder m nested-env lcaptured)
                                           (build-ret nested-builder v)))))
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
                                       (build-make-closure builder m nested-fn env-ptr env-len (compute-sexpr-mask lcaptured 0))))))))))
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
                         (if (sexpr-consp defs)
                             (let ((def (sexpr-car defs)) (rest (sexpr-cdr defs)))
                              (let ((nm (sexpr-str (sexpr-car def))))
                              (let ((mangled (append name (append "$" nm))))
                                (if (eq (sexpr-list-length captured) 0)
                                    (set inner-fn-env nm (add-function m mangled))
                                    (set inner-fn-env nm (add-function-with-env m mangled)))
                                (declare-labels-siblings inner-fn-env captured rest))))
                             ()))
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
                         (if (sexpr-consp defs)
                             (let ((def (sexpr-car defs)) (rest (sexpr-cdr defs)))
                              (let ((nm (sexpr-str (sexpr-car def))))
                              (let ((param-syms (sexpr-car (sexpr-cdr def))))
                                (let ((def-body (sexpr-car (sexpr-cdr (sexpr-cdr def)))))
                                  (match (get inner-fn-env nm)
                                    ((Some sib-fn)
                                     (let ((sib-block (append-block sib-fn "entry")))
                                       (let ((sib-builder (llvm-builder::create)))
                                         (position-at-end sib-builder sib-block)
                                         (let ((sib-env (new-env)))
                                           (bind-params sib-env sib-builder m sib-fn param-syms 0)
                                           (retain-bindings sib-builder m sib-env param-syms)
                                           ;; See `compile-lambda`'s matching
                                           ;; comment: no `retain-bindings`
                                           ;; call for `captured` — every
                                           ;; entry is `kind >= 10` now, and
                                           ;; `bind-captures` already roots
                                           ;; each one itself.
                                           (bind-captures sib-env sib-builder m sib-fn captured 0)
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
                                               (release-bindings sib-builder m sib-env param-syms)
                                               (release-bindings sib-builder m sib-env captured)
                                               (build-ret sib-builder v)
                                               (compile-labels-bodies inner-fn-env captured rest)))))))
                                    (None (panic (append "compile-labels-bodies: missing declaration for " nm))))))))
                             ()))
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
                         (let ((inner-captured (sexpr-car (sexpr-cdr e))))
                           (let ((defs (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
                             (let ((trailing (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
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
                         (let ((body-forms (sexpr-cdr e)))
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
                         (if (sexpr-consp forms)
                             (let ((form (sexpr-car forms)) (rest (sexpr-cdr forms)))
                              (let ((ignored (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form)))
                              (if (block-terminated? builder)
                                  ()
                                  (compile-loop-body builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base rest))))
                             ()))
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
                         (let ((is-fn (sexpr-bool (sexpr-car (sexpr-cdr e)))))
                           (let ((value-form (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
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
                         (let ((nm (sexpr-str (sexpr-car (sexpr-cdr e)))))
                           (let ((kind (sexpr-int (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
                             (let ((is-fn (eq kind 1)))
                               (let ((value-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
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
                       ;; `(cellset name kind value-form)` — `setf` on a
                       ;; cell-boxed name (closure-representation unification
                       ;; Stage 4): unlike `compile-set`, the target's own
                       ;; slot (word 0) is never touched again after
                       ;; `bind-params`/`bind-let-values`/`bind-captures`
                       ;; first populated it with the cell reference — this
                       ;; mutates the *cell itself* in place via
                       ;; `rt_cell_set`, so every other holder of that same
                       ;; `BoxedObj::Cell` (an outer scope's own binding, a
                       ;; sibling closure that captured it, the interpreter's
                       ;; own `Slot::Heap` if this cell originated there) sees
                       ;; the write immediately — the CL "shared mutable
                       ;; capture" semantics this whole stage exists to give.
                       ;; No `compile-if-branch`/borrowed-value handling is
                       ;; needed the way `compile-set` still nominally has:
                       ;; that machinery is inert since the ARC scheme it once
                       ;; served was removed (`compile-if-branch` is now just
                       ;; `compile-value`, see its own doc comment), so this
                       ;; calls `compile-value` directly. No transient
                       ;; `push-sexpr-root` around `compile-tag-struct-field`'s
                       ;; result is needed either, unlike `bind-params`'/
                       ;; `bind-let-values`' own cell-creation: `rt_cell_set`
                       ;; itself allocates nothing, so there is no allocating
                       ;; call for the freshly tagged value to need protecting
                       ;; across — it is consumed immediately, in the very
                       ;; next instruction. Returns `v` (the new, untagged
                       ;; value), matching `compile-set`'s own "a `setf`
                       ;; evaluates to the value that was set" convention.
                       (compile-cellset ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((nm (sexpr-str (sexpr-car (sexpr-cdr e)))))
                           (let ((kind (sexpr-int (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
                             (let ((value-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                               (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base value-form)))
                                 (match (get env nm)
                                   ((Some slot)
                                    (let ((cell-ref (load-raw builder slot 0)))
                                      (let ((tagged (compile-tag-struct-field builder m v kind)))
                                        (let ((args-ptr (alloca-args builder 2)))
                                          (store-arg builder args-ptr 0 cell-ref)
                                          (store-arg builder args-ptr 1 tagged)
                                          (let ((ignored (build-call builder (get-function m "rt_cell_set") args-ptr 2)))
                                            v)))))
                                   (None (panic (append "compile-cellset: unbound variable " nm)))))))))
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
                         (let ((s (sexpr-sym-name (sexpr-car pat))))
                            (if (equal s "pat-wild")
                                ()
                                (if (equal s "pat-bind")
                                    (let ((nm (sexpr-str (sexpr-car (sexpr-cdr pat)))))
                                      (let ((bslot (alloca-args builder 1)))
                                        (store-arg builder bslot 0 v)
                                        (set env nm bslot)))
                                    (if (equal s "pat-lit")
                                        (compile-pattern-guard builder cur-fn (build-icmp-eq builder v (const-i64 builder (sexpr-int (sexpr-car (sexpr-cdr pat))))) fail-block)
                                        (if (equal s "pat-ctor")
                                            (let ((variant (sexpr-int (sexpr-car (sexpr-cdr pat)))))
                                              (let ((subpats (sexpr-car (sexpr-cdr (sexpr-cdr pat)))))
                                                ;; `scrut-kind`: 0 = `Sexpr` (tagged-i64
                                                ;; tests), 1 = sum-ADT box (tests/extracts by
                                                ;; slot), 2 = boxed struct (`defstruct`/`Vector`
                                                ;; -- single variant, so no tag test at all,
                                                ;; only field extraction via `field-kinds`).
                                                (let ((scrut-kind (sexpr-int (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr pat)))))))
                                                  (let ((field-kinds (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr pat)))))))
                                                    (if (eq scrut-kind 2)
                                                        ()
                                                        (if (eq scrut-kind 1)
                                                            (compile-pattern-guard builder cur-fn (compile-box-tag-test builder m v variant) fail-block)
                                                            (compile-pattern-guard builder cur-fn (compile-sexpr-tag-test builder m v variant) fail-block)))
                                                    (compile-ctor-subpatterns builder env cur-fn v scrut-kind variant field-kinds subpats 0 fail-block)))))
                                            (panic (append "compile-pattern-test: unsupported pattern tag " s)))))))
                           )
                       ;; Tests/extracts each of a `pat-ctor`'s
                       ;; sub-patterns in turn against variant `variant`'s
                       ;; fields (a boxed struct's `field-kinds` list --
                       ;; meaningful only for `scrut-kind = 2` -- advances in
                       ;; lockstep with `subpats`/`idx`), skipping the
                       ;; extraction call entirely for a `pat-wild`
                       ;; sub-pattern (no value to test or bind, so no
                       ;; reason to call `compile-sexpr-field`/
                       ;; `compile-struct-field` -- and for `cons`, no
                       ;; reason to emit an `rt_car`/`rt_cdr` call either).
                       (compile-ctor-subpatterns ((builder llvm-builder) (env Scope<llvm-value>) (cur-fn llvm-function) (v llvm-value) (scrut-kind i64) (variant i64) (field-kinds Sexpr) (subpats Sexpr) (idx i32) (fail-block llvm-basic-block)) ()
                         (if (sexpr-consp subpats)
                             (let ((p (sexpr-car subpats)) (rest (sexpr-cdr subpats)))
                              (let ((rest-kinds (if (eq scrut-kind 0) field-kinds (sexpr-cdr field-kinds))))
                               (if (equal (sexpr-sym-name (sexpr-car p)) "pat-wild")
                                (compile-ctor-subpatterns builder env cur-fn v scrut-kind variant rest-kinds rest (+ idx 1) fail-block)
                                (let ((field-v (if (eq scrut-kind 2)
                                                    (compile-struct-field builder m v (sexpr-int (sexpr-car field-kinds)) idx)
                                                    (if (eq scrut-kind 1)
                                                        (compile-box-field builder m v (sexpr-int (sexpr-car field-kinds)) idx)
                                                        (compile-sexpr-field builder m v variant idx)))))
                                  (compile-pattern-test builder env cur-fn field-v p fail-block)
                                  (compile-ctor-subpatterns builder env cur-fn v scrut-kind variant rest-kinds rest (+ idx 1) fail-block)))))
                             ()))
                       ;; `(match is-fn scrutinee-form ((pattern-form .
                       ;; body-form)...) scrut-kind)` -- `Expr::Match`
                       ;; (`ast_bridge::translate_match`; any scrutinee type
                       ;; that isn't `Sexpr`/a sum-ADT box/a boxed struct
                       ;; stays `unsupported`). Compiles the scrutinee
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
                       ;; demonstrates directly: for a `Sexpr` or boxed-struct
                       ;; scrutinee (`scrut-kind` 0/2 — both ordinary
                       ;; GC-managed heap values), unlike a `let` binding or
                       ;; call argument, nothing else ever rooted it — a
                       ;; *fresh* scrutinee (e.g. the direct result of a call,
                       ;; never bound to a name) had no owner at all, so a
                       ;; `pat-bind` field extracted from it
                       ;; (`compile-pattern-test`'s own `bslot`, itself never
                       ;; rooted either) was only ever safe by the GC
                       ;; transitively marking it *through* `scrut-v` — which
                       ;; nothing protected an allocation during a later arm
                       ;; statement (e.g. a sibling `let` binding's own value)
                       ;; from reclaiming outright. Rooting `scrut-v` for the
                       ;; match's own duration keeps that transitive
                       ;; reachability valid the whole time, which is enough:
                       ;; nothing here needs to root each individual
                       ;; `pat-bind` separately. A sum-ADT box (`scrut-kind`
                       ;; 1) needs none of this — it isn't GC-managed (never
                       ;; scanned or freed) and its `Sexpr` fields are
                       ;; permanently rooted from construction. Popped only
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
                       ;; nested between the exit site and the loop. A
                       ;; `scrut-kind = 1` (enum) scrutinee needs this exact
                       ;; same root protection since the enum-representation
                       ;; unification's compiler flip — it's a real
                       ;; GC-managed `BoxedObj::Enum` now, no different from
                       ;; a `Sexpr`/boxed-struct scrutinee — so the old
                       ;; `scrut-kind = 1` exclusion below is gone; all three
                       ;; kinds root/unroot identically.
                       (compile-match ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((is-fn (sexpr-bool (sexpr-car (sexpr-cdr e)))))
                           (let ((scrut-form (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
                             (let ((arms (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                               ;; The node's trailing `scrut-kind` field
                               ;; (`ast_bridge::translate_match`) isn't read
                               ;; here — every scrutinee kind now roots
                               ;; identically (see this function's own doc
                               ;; comment) — only `compile-pattern-test`'s
                               ;; *per-pattern* `scrut-kind` (embedded in each
                               ;; `pat-ctor`, a separate field) still matters.
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
                         (if (sexpr-consp arms)
                             (let ((arm (sexpr-car arms)) (rest (sexpr-cdr arms)))
                              (let ((pat (sexpr-car arm)))
                              (let ((body-form (sexpr-cdr arm)))
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
                             (let ((args-ptr (alloca-args builder 0)))
                                (let ((fallback (build-call builder (get-function m "rt_match_fail") args-ptr 0)))
                                  (store-arg builder slot 0 fallback)
                                  (build-br builder merge-block)))))
                       ;; `(construct is-sexpr mutable type-name-str
                       ;; variant-i64 arg-form...)` (Stage 6 of the
                       ;; Sexpr-representation plan, extended by Stage 3 of
                       ;; the Sexpr/RtValue unification plan —
                       ;; `docs/implementation-log.md` — with `mutable`/
                       ;; `type-name-str`) — `is-sexpr`/`mutable`
                       ;; (`ast_bridge::is_sexpr_type`/`translate_construct`'s
                       ;; own `mutable` parameter, read off `Expr::Construct`'s
                       ;; field) together dispatch between `Sexpr`'s own 8
                       ;; variants (`compile-construct-sexpr`), a `mutable`
                       ;; `defstruct`/`Vector<T>`/`cons-cell<K,V>`'s
                       ;; GC-tracked boxed-struct representation
                       ;; (`compile-construct-boxed-struct`), and every other
                       ;; ADT's general `malloc`'d-box representation
                       ;; (`compile-construct-box`) — see those three
                       ;; functions' doc comments.
                       ;; `variant` `5`/`10` (quoted `Sym`/`Path` —
                       ;; `ast_bridge::translate_quote`'s doc comment) are
                       ;; peeled off *here*, before delegating to
                       ;; `compile-construct-sexpr`, rather than folded into
                       ;; that function's own already-deep dispatch chain —
                       ;; adding even one more arm there was enough to
                       ;; overflow an unrelated test's stack (`scripts/
                       ;; test-serial.sh`'s `RUST_MIN_STACK` comment records
                       ;; the incident), since `Checker::check_if`/
                       ;; `Interp::eval` never loopified `if`-chain recursion
                       ;; the way `compile-value`'s own dispatch did.
                       (compile-construct ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((is-sexpr (sexpr-bool (sexpr-car (sexpr-cdr e)))))
                           (let ((mutable (sexpr-bool (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
                             (let ((type-name-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                               (let ((variant (sexpr-int (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))))
                                 (let ((arg-forms (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))
                                   (if (eq variant 5)
                                       (compile-construct-sym builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base arg-forms)
                                       (if (eq variant 10)
                                           (compile-construct-path builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base arg-forms)
                                           (if is-sexpr
                                               (compile-construct-sexpr builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base variant arg-forms)
                                               (if mutable
                                                   (compile-construct-boxed-struct builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base type-name-form arg-forms)
                                                   (compile-construct-box builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base type-name-form variant arg-forms)))))))))))
                       ;; `(construct true false empty 5 name-form)` — a
                       ;; quoted symbol literal (`ast_bridge::translate_quote`'s
                       ;; `Sym` arm). `name-form` is an ordinary `(str ...)`
                       ;; node (`str_literal_form`); compiling it yields a
                       ;; fully tagged `Sexpr::Str`, handed straight to
                       ;; `rt_intern_symbol` (no GC-root protection needed —
                       ;; an interned symbol is permanent, unlike the `Str`
                       ;; that briefly holds its name).
                       (compile-construct-sym ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (arg-forms Sexpr)) llvm-value
                         (let ((name-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-car arg-forms))))
                           (let ((args-ptr (alloca-args builder 1)))
                             (store-arg builder args-ptr 0 name-v)
                             (build-call builder (get-function m "rt_intern_symbol") args-ptr 1))))
                       ;; `(construct true false empty 10 seg-form...)` — a
                       ;; quoted `::`-path literal (`ast_bridge::translate_quote`'s
                       ;; `Path` arm), one `(str ...)` node per segment.
                       ;; `compile-construct-path-segs` interns each segment
                       ;; (`rt_intern_symbol`, same as `compile-construct-sym`)
                       ;; into a fresh `args-ptr` array, then `rt_intern_path`
                       ;; combines them into the final tagged `Sexpr::Path`.
                       (compile-construct-path ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (arg-forms Sexpr)) llvm-value
                         (let ((n (sexpr-list-length arg-forms)))
                           (let ((args-ptr (alloca-args builder n)))
                             (compile-construct-path-segs builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr arg-forms 0)
                             (build-call builder (get-function m "rt_intern_path") args-ptr n))))
                       ;; Fills a `compile-construct-path`-allocated array,
                       ;; one interned segment `Sexpr::Symbol` per slot —
                       ;; the path-literal analogue of `store-str-chars`.
                       (compile-construct-path-segs ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (args-ptr llvm-value) (forms Sexpr) (idx i32)) ()
                         (if (sexpr-consp forms)
                             (let ((form (sexpr-car forms)) (rest (sexpr-cdr forms)))
                               (let ((name-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form)))
                                 (let ((seg-args-ptr (alloca-args builder 1)))
                                   (store-arg builder seg-args-ptr 0 name-v)
                                   (let ((sym-v (build-call builder (get-function m "rt_intern_symbol") seg-args-ptr 1)))
                                     (store-arg builder args-ptr idx sym-v)
                                     (compile-construct-path-segs builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr rest (+ idx 1))))))
                             ()))
                       ;; Builds a `BoxedObj::Struct` (Stage 3 of the
                       ;; Sexpr/RtValue unification plan,
                       ;; `docs/implementation-log.md`) for a `mutable`
                       ;; `defstruct`/`Vector<T>`/`cons-cell<K,V>` instance —
                       ;; the same tagged `Value::Boxed` representation the
                       ;; interpreter's own `Expr::Construct` `mutable` arm
                       ;; already builds (`heap.alloc_struct`), so a value
                       ;; either side constructs interoperates with the
                       ;; other (unlike `compile-construct-box`'s
                       ;; never-GC-tracked `malloc`'d array). `type-name-form`
                       ;; is `translate_construct`'s own `(str (int c)...)`
                       ;; literal form for the struct's local type name
                       ;; (`str_literal_form` — never a pre-allocated
                       ;; `Value::Str`, since AOT's target program shares no
                       ;; `Heap`/`StrId` table with this one), compiled once
                       ;; and passed as `rt_struct_new`'s own `args[0]`
                       ;; (that function's doc comment: a tagged `Sexpr`
                       ;; `Str`, exactly what `compile-str` already produces).
                       ;; Each remaining field is compiled then re-tagged by
                       ;; `compile-tag-struct-field` per its own
                       ;; `ast_bridge::struct_field_kind`
                       ;; (`compile-construct-boxed-struct-fields`) before
                       ;; being handed to `rt_struct_new`, which expects
                       ;; every argument after the type name to already be a
                       ;; properly tagged `Sexpr` (that function's own
                       ;; `decode()` call on each). The freshly built struct
                       ;; is `push-permanent-sexpr-root`ed immediately —
                       ;; unlike a `let`/parameter binding's own scope-based
                       ;; `kind = 2` rooting (`ast_bridge::binding_kind`
                       ;; deliberately doesn't classify a `mutable` struct
                       ;; type as `KIND_SEXPR`: this permanent root already
                       ;; keeps every boxed struct alive from birth, so
                       ;; per-binding push/pop would only add redundant
                       ;; bookkeeping to every scope the value crosses),
                       ;; a permanent root is the same accepted-leak
                       ;; treatment `compile-construct-box-fields` already
                       ;; gives a general-ADT box's own `Sexpr`-typed fields,
                       ;; just applied to the struct itself rather than one
                       ;; of its fields — sidesteps needing to track this
                       ;; struct's own binding scope at all.
                       (compile-construct-boxed-struct ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (type-name-form Sexpr) (arg-forms Sexpr)) llvm-value
                         (let ((argc (sexpr-list-length arg-forms)))
                           (let ((args-ptr (alloca-args builder (+ argc 1))))
                             (let ((name-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base type-name-form)))
                               (store-arg builder args-ptr 0 name-v)
                               (compile-construct-boxed-struct-fields builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr arg-forms 1)
                               (let ((result (build-call builder (get-function m "rt_struct_new") args-ptr (+ argc 1))))
                                 (push-permanent-sexpr-root builder m result)
                                 result)))))
                       ;; Fills a `compile-construct-boxed-struct`-allocated
                       ;; argument array, one compiled-then-tagged field per
                       ;; slot starting at `idx` (`1`, skipping the type-name
                       ;; slot `rt_struct_new`'s own `args[0]` occupies) —
                       ;; the boxed-struct analogue of
                       ;; `compile-construct-box-fields`. Each `forms`
                       ;; element is a `(kind . field-form)` pair
                       ;; (`ast_bridge::struct_field_ast_list_to_sexpr`,
                       ;; tagged by `ast_bridge::struct_field_kind` rather
                       ;; than `binding_kind`); `compile-tag-struct-field`
                       ;; turns the field's own compiled (untagged, for a
                       ;; scalar kind) value into the tagged `Sexpr`
                       ;; `rt_struct_new` requires.
                       (compile-construct-boxed-struct-fields ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (args-ptr llvm-value) (forms Sexpr) (idx i32)) ()
                         (if (sexpr-consp forms)
                             (let ((field-pair (sexpr-car forms)) (rest (sexpr-cdr forms)))
                              (let ((kind (sexpr-int (sexpr-car field-pair))))
                              (let ((form (sexpr-cdr field-pair)))
                                (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base form)))
                                  (let ((tagged-v (compile-tag-struct-field builder m v kind)))
                                    (store-arg builder args-ptr idx tagged-v)
                                    (compile-construct-boxed-struct-fields builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr rest (+ idx 1)))))))
                             ()))
                       ;; Builds a boxed enum value (`Option`/`Result`/a
                       ;; user `defenum`) — the enum-representation
                       ;; unification's compiler flip: a real `BoxedObj::Enum`
                       ;; via `rt_data_new` now, not the un-GC-managed
                       ;; `build-malloc`'d array this used to leak. Mirrors
                       ;; `compile-construct-boxed-struct` exactly, with one
                       ;; extra header slot: `args[0]` the type name (built
                       ;; from `type-name-form`, the same `str_literal_form`
                       ;; `translate_construct` now builds for every non-
                       ;; `Sexpr` construct, not just a `mutable` one),
                       ;; `args[1]` the raw variant index, `args[2 + i]` the
                       ;; `i`-th field — reusing
                       ;; `compile-construct-boxed-struct-fields` unchanged to
                       ;; fill those (starting at `2` instead of `1`), since
                       ;; `ast_bridge::translate_construct`'s enum branch now
                       ;; tags its fields by `struct_field_kind`
                       ;; (`struct_field_ast_list_to_sexpr`) exactly like the
                       ;; `mutable` branch does — every enum type compiled
                       ;; code can ever mention is heap-repr by construction
                       ;; (see `ast_bridge::struct_field_kind`'s doc comment),
                       ;; so there is no longer a distinct "general-ADT field"
                       ;; tagging scheme to keep separate. The freshly built
                       ;; enum value is `push-permanent-sexpr-root`ed
                       ;; immediately, the same accepted-forever-alive
                       ;; treatment `compile-construct-boxed-struct` already
                       ;; gives its own result (not a leak: a real GC-managed
                       ;; value now, just never released early).
                       (compile-construct-box ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (type-name-form Sexpr) (variant i64) (arg-forms Sexpr)) llvm-value
                         (let ((argc (sexpr-list-length arg-forms)))
                           (let ((args-ptr (alloca-args builder (+ argc 2))))
                             (let ((name-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base type-name-form)))
                               (store-arg builder args-ptr 0 name-v)
                               (store-arg builder args-ptr 1 (const-i64 builder variant))
                               (compile-construct-boxed-struct-fields builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base args-ptr arg-forms 2)
                               (let ((result (build-call builder (get-function m "rt_data_new") args-ptr (+ argc 2))))
                                 (push-permanent-sexpr-root builder m result)
                                 result)))))
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
                       ;; value passed straight through. `float`'s `f64`
                       ;; field (Sexpr/RtValue unification, Stage 0) is
                       ;; boxed via `rt_float_new` (`typelisp-rt`'s
                       ;; `TAG_BOXED`) rather than any bit manipulation here
                       ;; — `f64` doesn't fit alongside a 3-bit tag the way
                       ;; `int`/`char`/`bool` do. The field's own compiled
                       ;; value is already the raw `f64` bit pattern
                       ;; (`f64::to_bits`), the convention `rt_float_new`
                       ;; expects. `sym`'s `Str` field still isn't
                       ;; representable in compiled code yet (a `Sym`'s tag
                       ;; payload is a `SymId`, a separate gap from `str`'s
                       ;; own — see `compile-sexpr-field`'s doc comment), so
                       ;; constructing one still panics clearly.
                       (compile-construct-sexpr ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (variant i64) (arg-forms Sexpr)) llvm-value
                         (if (eq variant 0)
                             (const-i64 builder 6)
                             (if (eq variant 1)
                                 (build-shl builder (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-car arg-forms)) (const-i64 builder 3))
                                 (if (eq variant 2)
                                     (let ((args-ptr (alloca-args builder 1)))
                                       (store-arg builder args-ptr 0 (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-car arg-forms)))
                                       (build-call builder (get-function m "rt_float_new") args-ptr 1))
                                 (if (eq variant 3)
                                     (build-or builder
                                                (build-shl builder (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-car arg-forms)) (const-i64 builder 3))
                                                (const-i64 builder 4))
                                     (if (eq variant 4)
                                         (let ((b (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-car arg-forms))))
                                           (build-or builder (build-shl builder (build-add builder b (const-i64 builder 1)) (const-i64 builder 3)) (const-i64 builder 6)))
                                         (if (eq variant 7)
                                             (let ((args-ptr (alloca-args builder 2)))
                                               (let ((car-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-car arg-forms))))
                                                 (store-arg builder args-ptr 0 car-v)
                                                 (push-sexpr-root builder m car-v)
                                                 (let ((cdr-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-car (sexpr-cdr arg-forms)))))
                                                   (store-arg builder args-ptr 1 cdr-v)
                                                   (push-sexpr-root builder m cdr-v)
                                                   (let ((result (build-call builder (get-function m "rt_cons") args-ptr 2)))
                                                     (pop-sexpr-root builder m)
                                                     (pop-sexpr-root builder m)
                                                     result))))
                                             (if (eq variant 6)
                                                 (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-car arg-forms))
                                                 ;; `bignum`(8)/`ratio`(9): same "already fully tagged, no
                                                 ;; further bit manipulation" passthrough as `str`(6) above —
                                                 ;; the field's own compiled form is `compile-bignum-literal`/
                                                 ;; `compile-ratio-literal`'s `rt_bignum_new`/
                                                 ;; `rt_ratio_from_bignums` call result, already a proper
                                                 ;; `TAG_BOXED` `Sexpr::Bignum`/`Ratio`.
                                                 (if (if (eq variant 8) true (eq variant 9))
                                                     (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-car arg-forms))
                                                     (panic "compile-construct-sexpr: field type is not representable in compiled code yet"))))))))))
                       ;; `(field-get idx-unary-list kind-i64 obj-form)`
                       ;; (Stage 6, extended by Stage 3 of the Sexpr/RtValue
                       ;; unification plan — `docs/implementation-log.md` —
                       ;; with `kind-i64`) — always a `BoxedObj::Struct`
                       ;; `compile-construct-boxed-struct` built (this tag is
                       ;; only ever synthesized by `Checker::check_defstruct`
                       ;; as a field accessor's own body, never for a `Sexpr`
                       ;; — see `ast_bridge::translate_field_get`'s doc
                       ;; comment, and every `defstruct` is `mutable`): the
                       ;; object's own tagged value is handed straight to
                       ;; `rt_struct_field_get` (which decodes/re-indexes it
                       ;; itself, no `build-int-to-ptr` needed the way a
                       ;; general-ADT box's raw pointer required), and the
                       ;; raw `Sexpr` it returns is decoded back into this
                       ;; field's own compiled representation by
                       ;; `compile-sexpr-field` (reused verbatim — `kind`
                       ;; *is* `ast_bridge::struct_field_kind`'s value, the
                       ;; same `Sexpr`-variant numbering `compile-sexpr-field`
                       ;; already expects; its own `idx` parameter is unused
                       ;; for every kind this can produce, so `0` is passed).
                       ;; `idx` (the *field* index, unrelated to `kind`) is
                       ;; recovered from `idx-unary-list`'s own length
                       ;; (`ast_bridge::idx_unary_list`'s doc comment
                       ;; explains why it isn't simply a `Sexpr` `Int`) — no
                       ;; header offset here, unlike the general-ADT box
                       ;; layout's own variant-tag slot: a `BoxedObj::Struct`'s
                       ;; own field vector has no variant-tag slot of its own.
                       (compile-field-get ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((idx (sexpr-list-length-i64 (sexpr-car (sexpr-cdr e)))))
                           (let ((kind (sexpr-int (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
                             (let ((obj-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                               (let ((obj-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base obj-form)))
                                 (let ((args-ptr (alloca-args builder 2)))
                                   (store-arg builder args-ptr 0 obj-v)
                                   (store-arg builder args-ptr 1 (const-i64 builder idx))
                                   (let ((raw (build-call builder (get-function m "rt_struct_field_get") args-ptr 2)))
                                     (compile-sexpr-field builder m raw kind 0))))))))
                       ;; `(field-set idx-unary-list kind-i64 obj-form
                       ;; value-form)` (Stage 6, extended the same way
                       ;; `compile-field-get` was) — see that function's doc
                       ;; comment; `compile-tag-struct-field` (the encode-side
                       ;; mirror of `compile-sexpr-field`'s decode) tags the
                       ;; already-compiled `value-form` before
                       ;; `rt_struct_field_set` stores it. Evaluates to `Unit`
                       ;; (`0`, `compile-unit`'s own convention), matching
                       ;; `Expr::FieldSet`'s own checked type.
                       (compile-field-set ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((idx (sexpr-list-length-i64 (sexpr-car (sexpr-cdr e)))))
                           (let ((kind (sexpr-int (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
                             (let ((obj-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                               (let ((value-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))
                                 (let ((obj-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base obj-form)))
                                   (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base value-form)))
                                     (let ((tagged-v (compile-tag-struct-field builder m v kind)))
                                       (let ((args-ptr (alloca-args builder 3)))
                                         (store-arg builder args-ptr 0 obj-v)
                                         (store-arg builder args-ptr 1 (const-i64 builder idx))
                                         (store-arg builder args-ptr 2 tagged-v)
                                         (let ((ignored (build-call builder (get-function m "rt_struct_field_set") args-ptr 3)))
                                           (const-i64 builder 0)))))))))))
                       ;; `(vector-op method kind v-form arg-form...)` — a
                       ;; `Vector<T>` builtin method (`ast_bridge`'s
                       ;; `translate_vector_method`), lowered here rather than
                       ;; through `compile-assoc` because these have no
                       ;; compiled `defmethod` body. `kind` is `T`'s
                       ;; `struct_field_kind`; the vector itself is a boxed
                       ;; struct (tagged `Sexpr`), so its elements cross the
                       ;; `BoxedObj::Struct` boundary tagged and need the same
                       ;; `compile-tag-struct-field` (encode, `push`/`set`) /
                       ;; `compile-sexpr-field` (decode, `get`) `compile-field-
                       ;; set`/`compile-field-get` use for a fixed-arity field
                       ;; — the sole difference being a *runtime* index
                       ;; (compiled from `idx-form`, already a raw `i64`)
                       ;; rather than a compile-time-constant field offset.
                       ;; `len`/`push` are the two new primitives:
                       ;; `rt_struct_field_count` (raw element count) and
                       ;; `rt_struct_push_field` (grow by one). An out-of-range
                       ;; `get`/`set` index aborts inside `rt_struct_field_get`/
                       ;; `_set` (the mem-layer bounds panic across the
                       ;; `extern "C"` boundary), matching compiled code's
                       ;; "abort, don't unwind" convention (`rt_panic`) — the
                       ;; one intentional semantic gap from the interpreter,
                       ;; which recovers the same overrun as an `EvalError`.
                       ;; No extra GC-rooting beyond what `compile-field-set`
                       ;; already relies on: the receiver flows in as an
                       ;; ordinary (env-rooted) value, and `rt_struct_push_
                       ;; field` allocates nothing.
                       (compile-vector-op ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((method (sexpr-str (sexpr-car (sexpr-cdr e)))))
                           (let ((kind (sexpr-int (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
                             ;; `new` reuses the boxed-struct construct path: an
                             ;; empty `"vector"` struct (the type-name form is
                             ;; the node's lone operand, no fields).
                             (if (equal method "new")
                                 (compile-construct-boxed-struct builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))) (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
                             (let ((v-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                               (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base v-form)))
                                 (if (equal method "len")
                                     (let ((args-ptr (alloca-args builder 1)))
                                       (store-arg builder args-ptr 0 v)
                                       (build-call builder (get-function m "rt_struct_field_count") args-ptr 1))
                                     (if (equal method "push")
                                         (let ((x-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))
                                           (let ((x (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base x-form)))
                                             (let ((tagged-x (compile-tag-struct-field builder m x kind)))
                                               (let ((args-ptr (alloca-args builder 2)))
                                                 (store-arg builder args-ptr 0 v)
                                                 (store-arg builder args-ptr 1 tagged-x)
                                                 (let ((ignored (build-call builder (get-function m "rt_struct_push_field") args-ptr 2)))
                                                   (const-i64 builder 0))))))
                                         (let ((idx-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))
                                           (let ((idx (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base idx-form)))
                                             (if (equal method "get")
                                                 (let ((args-ptr (alloca-args builder 2)))
                                                   (store-arg builder args-ptr 0 v)
                                                   (store-arg builder args-ptr 1 idx)
                                                   (let ((raw (build-call builder (get-function m "rt_struct_field_get") args-ptr 2)))
                                                     (compile-sexpr-field builder m raw kind 0)))
                                                 (let ((x-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))))
                                                   (let ((x (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base x-form)))
                                                     (let ((tagged-x (compile-tag-struct-field builder m x kind)))
                                                       (let ((args-ptr (alloca-args builder 3)))
                                                         (store-arg builder args-ptr 0 v)
                                                         (store-arg builder args-ptr 1 idx)
                                                         (store-arg builder args-ptr 2 tagged-x)
                                                         (let ((ignored (build-call builder (get-function m "rt_struct_field_set") args-ptr 3)))
                                                           (const-i64 builder 0)))))))))))))))))
                       ;; `(hashtable-op method key-kind val-kind ht-form
                       ;; ...)` — a `HashTable<K,V>` builtin (`ast_bridge`'s
                       ;; `translate_hashtable_method`), lowered to the
                       ;; `rt_hashtable_*` family (the mem layer owns the key
                       ;; hashing). `new` builds an empty map (no operands);
                       ;; `set` tags its key/value by `key-kind`/`val-kind`
                       ;; (`compile-tag-struct-field`, as `compile-field-set`
                       ;; does) before the mem layer hashes the key; `count`
                       ;; returns a raw `i64`; `clear` returns `Unit`; and
                       ;; `keys`/`values`/`entries` each build a fresh
                       ;; `Vector` (`entries`'s of `cons-cell`s) that — like a
                       ;; `compile-construct-boxed-struct` result — is
                       ;; `push-permanent-sexpr-root`ed on the spot so a later
                       ;; allocation can't reclaim it. `get`/`remove` are not
                       ;; here: their `Option` return is a `malloc`'d sum-ADT
                       ;; box, a separate problem from iteration (see
                       ;; `docs/dev/iter-compile-plan.md`), so they fall
                       ;; through to `compile-assoc` and panic clearly.
                       (compile-hashtable-op ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((method (sexpr-str (sexpr-car (sexpr-cdr e)))))
                           (if (equal method "new")
                               (let ((args-ptr (alloca-args builder 1)))
                                 (let ((result (build-call builder (get-function m "rt_hashtable_new") args-ptr 0)))
                                   (push-permanent-sexpr-root builder m result)
                                   result))
                               (let ((key-kind (sexpr-int (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
                                 (let ((val-kind (sexpr-int (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))
                                   (let ((option-type-name-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))
                                   (let ((ht-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))))
                                     (let ((ht (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base ht-form)))
                                       (if (equal method "set")
                                           (let ((key-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))))
                                             (let ((val-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))))))
                                               (let ((k (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base key-form)))
                                                 (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base val-form)))
                                                   (let ((tk (compile-tag-struct-field builder m k key-kind)))
                                                     (let ((tv (compile-tag-struct-field builder m v val-kind)))
                                                       (let ((args-ptr (alloca-args builder 3)))
                                                         (store-arg builder args-ptr 0 ht)
                                                         (store-arg builder args-ptr 1 tk)
                                                         (store-arg builder args-ptr 2 tv)
                                                         (let ((ignored (build-call builder (get-function m "rt_hashtable_set") args-ptr 3)))
                                                           (const-i64 builder 0)))))))))
                                           (if (if (equal method "get") true (equal method "remove"))
                                               ;; `get`/`remove`: a runtime lookup whose found/not-found
                                               ;; outcome decides *which sum-ADT variant* to build, so
                                               ;; (unlike an ordinary `Option::some`/`none` source call,
                                               ;; which `compile-construct-box` builds from a
                                               ;; compile-time-known variant) this can't reuse that
                                               ;; function directly — it needs real control flow. Follows
                                               ;; `compile-if`'s own "alloca a merge slot, branch, store
                                               ;; each arm's result, load after the merge block" shape
                                               ;; (no `phi` builtin exists here) rather than a new one.
                                               ;; `rt_hashtable_contains` is checked first (a `mem::Value`'s
                                               ;; tag space has no free bit pattern to serve as a "not
                                               ;; found" sentinel from a single value-returning call), then
                                               ;; only the confirmed-present branch calls
                                               ;; `rt_hashtable_get_raw`/`_remove_raw` — safe with no race,
                                               ;; single-threaded compiled code can't remove the entry
                                               ;; between the two calls. The found value, still tagged, is
                                               ;; decoded into the `Option` box's own "verbatim compiled
                                               ;; value" field convention via `compile-sexpr-field` — the
                                               ;; exact same decode a `BoxedObj::Struct` field read already
                                               ;; uses. `Some`/`None` build a real `BoxedObj::Enum` via
                                               ;; `rt_data_new` now (the enum-representation unification's
                                               ;; compiler flip) — `option-name-v` (compiled once, from
                                               ;; `option-type-name-form`, before the branch so both arms
                                               ;; can use the same SSA value) plus the raw variant index,
                                               ;; so `Interp::call_compiled` decodes the result exactly as
                                               ;; it already does for a source-level `Option::some`/`none`.
                                               (let ((key-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))))
                                                 (let ((k (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base key-form)))
                                                   (let ((tk (compile-tag-struct-field builder m k key-kind)))
                                                     (let ((option-name-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base option-type-name-form)))
                                                     (let ((lookup-args (alloca-args builder 2)))
                                                       (store-arg builder lookup-args 0 ht)
                                                       (store-arg builder lookup-args 1 tk)
                                                       (let ((found (build-call builder (get-function m "rt_hashtable_contains") lookup-args 2)))
                                                         (let ((then-block (append-block cur-fn "ht-found")))
                                                           (let ((else-block (append-block cur-fn "ht-not-found")))
                                                             (let ((merge-block (append-block cur-fn "ht-merge")))
                                                               (let ((slot (alloca-args builder 1)))
                                                                 (build-cond-br builder found then-block else-block)
                                                                 (position-at-end builder then-block)
                                                                 (let ((raw-fn (if (equal method "get") "rt_hashtable_get_raw" "rt_hashtable_remove_raw")))
                                                                   (let ((raw (build-call builder (get-function m raw-fn) lookup-args 2)))
                                                                     ;; `raw` is already the properly tagged `Sexpr`
                                                                     ;; the map stores (`set`'s own `compile-tag-
                                                                     ;; struct-field` call tagged it going in) —
                                                                     ;; `rt_data_new`'s field-value contract (like
                                                                     ;; `rt_struct_new`'s) wants exactly that, not a
                                                                     ;; decoded/untagged value, so this passes `raw`
                                                                     ;; straight through with no `compile-sexpr-
                                                                     ;; field` decode step.
                                                                     (let ((some-args (alloca-args builder 3)))
                                                                       (store-arg builder some-args 0 option-name-v)
                                                                       (store-arg builder some-args 1 (const-i64 builder 0))
                                                                       (store-arg builder some-args 2 raw)
                                                                       (let ((some-box (build-call builder (get-function m "rt_data_new") some-args 3)))
                                                                         (push-permanent-sexpr-root builder m some-box)
                                                                         (let ((ignored (store-arg builder slot 0 some-box)))
                                                                           (build-br builder merge-block))))))
                                                                 (position-at-end builder else-block)
                                                                 (let ((none-args (alloca-args builder 2)))
                                                                   (store-arg builder none-args 0 option-name-v)
                                                                   (store-arg builder none-args 1 (const-i64 builder 1))
                                                                   (let ((none-box (build-call builder (get-function m "rt_data_new") none-args 2)))
                                                                     (push-permanent-sexpr-root builder m none-box)
                                                                     (let ((ignored (store-arg builder slot 0 none-box)))
                                                                       (build-br builder merge-block))))
                                                                 (position-at-end builder merge-block)
                                                                 (load-raw builder slot 0)))))))))))
                                           (if (equal method "count")
                                               (let ((args-ptr (alloca-args builder 1)))
                                                 (store-arg builder args-ptr 0 ht)
                                                 (build-call builder (get-function m "rt_hashtable_count") args-ptr 1))
                                               (if (equal method "clear")
                                                   (let ((args-ptr (alloca-args builder 1)))
                                                     (store-arg builder args-ptr 0 ht)
                                                     (let ((ignored (build-call builder (get-function m "rt_hashtable_clear") args-ptr 1)))
                                                       (const-i64 builder 0)))
                                                   (let ((args-ptr (alloca-args builder 1)))
                                                     (store-arg builder args-ptr 0 ht)
                                                     (let ((result (build-call builder (get-function m (append "rt_hashtable_" method)) args-ptr 1)))
                                                       (push-permanent-sexpr-root builder m result)
                                                       result))))))))))))))
                       ;; `(global id kind)` — `id` is `path`'s already-
                       ;; promoted compiled-global slot
                       ;; (`Interp::add_compiled_function`'s
                       ;; `collect_global_targets`/`promote_global` step,
                       ;; `typelisp_rt::global_new`) — a permanent GC root, so
                       ;; unlike `compile-var`'s locally-bound name this needs
                       ;; no `env`/`fn-env` lookup at all, just a direct
                       ;; `rt_global_get` call by id. The permanent root
                       ;; always holds a properly *tagged* `Sexpr` (whatever
                       ;; `Interp::promote_global` produced), so `kind`
                       ;; (`ast_bridge::struct_field_kind`'s numbering — the
                       ;; same one `compile-field-get` already uses) untags
                       ;; it back to the global's own declared representation
                       ;; via `compile-sexpr-field` (this function's own
                       ;; decode step, shared with `compile-field-get`). An
                       ;; enum-typed global (`kind = 6`, since the enum-
                       ;; representation unification retired the old
                       ;; shift-tagged `kind = 10` special case) is just
                       ;; another passthrough value here, no different from
                       ;; a boxed struct or `Str` global.
                       (compile-global ((builder llvm-builder) (e Sexpr)) llvm-value
                         (let ((id (sexpr-int (sexpr-car (sexpr-cdr e)))))
                           (let ((kind (sexpr-int (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
                             (let ((args-ptr (alloca-args builder 1)))
                               (store-arg builder args-ptr 0 (const-i64 builder id))
                               (let ((raw (build-call builder (get-function m "rt_global_get") args-ptr 1)))
                                 (compile-sexpr-field builder m raw kind 0))))))
                       ;; `(set-global id kind value-form)` — `rt_global_set`
                       ;; overwrites the global's permanent root in place, so
                       ;; (unlike `compile-set`'s local-binding case) there is
                       ;; no separate GC root to keep in sync. `kind` (see
                       ;; `compile-global`'s doc comment) tags `value-form`'s
                       ;; own compiled result via `compile-tag-struct-field`
                       ;; (`compile-sexpr-field`'s encode-side mirror, shared
                       ;; with `compile-field-set`) before it's stored — a
                       ;; `Fn`-typed `value-form` isn't representable this
                       ;; way yet (`compile-tag-struct-field`'s own `kind = 0`
                       ;; case), so it panics clearly here rather than
                       ;; storing something `compile-global`'s own decode
                       ;; couldn't read back correctly. Evaluates to `v`
                       ;; itself (the newly stored value in its own,
                       ;; already-untagged compiled representation),
                       ;; matching `Expr::SetGlobal`'s own checked type.
                       (compile-set-global ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((id (sexpr-int (sexpr-car (sexpr-cdr e)))))
                           (let ((kind (sexpr-int (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
                             (let ((value-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                               (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base value-form)))
                                 (let ((tagged-v (compile-tag-struct-field builder m v kind)))
                                   (let ((args-ptr (alloca-args builder 2)))
                                     (store-arg builder args-ptr 0 (const-i64 builder id))
                                     (store-arg builder args-ptr 1 tagged-v)
                                     (let ((ignored (build-call builder (get-function m "rt_global_set") args-ptr 2)))
                                       v))))))))
                       ;; `(global-init kind value-form)` — AOT's synthesized
                       ;; startup sequence's own step, one per promoted global
                       ;; (`Interp::add_compiled_global_init`,
                       ;; `compile::aot`): unlike `compile-set-global`, there
                       ;; is no existing slot to overwrite yet, so this calls
                       ;; `rt_global_new` (one argument, no `id`) instead of
                       ;; `rt_global_set` — the id it allocates is implicit in
                       ;; call order (see `typelisp_rt::rt_global_new`'s doc
                       ;; comment), which is exactly why `compile::aot` must
                       ;; emit these calls, from the generated `main`, in the
                       ;; same order `Interp::promote_global` assigned
                       ;; compile-time ids in. The allocated id itself is
                       ;; discarded here (nothing at this call site needs it);
                       ;; this tag exists purely for its side effect.
                       (compile-global-init ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((kind (sexpr-int (sexpr-car (sexpr-cdr e)))))
                           (let ((value-form (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
                             (let ((v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base value-form)))
                               (let ((tagged-v (compile-tag-struct-field builder m v kind)))
                                 (let ((args-ptr (alloca-args builder 1)))
                                   (store-arg builder args-ptr 0 tagged-v)
                                   (build-call builder (get-function m "rt_global_new") args-ptr 1)))))))
                       ;; `(panic msg-form)` — `Expr::Panic`, always
                       ;; `Str`-typed (`Checker::check_panic` requires it), so
                       ;; no `kind`/scrutinee-shape dispatch is needed the way
                       ;; `Match`/`FieldGet` need — `msg-form` compiles down to
                       ;; the exact tagged `Sexpr::Str` representation
                       ;; `compile-str` already produces, handed straight to
                       ;; `rt_panic`, which prints it (`"panic: {msg}"`,
                       ;; matching `EvalError::Panic`'s own interpreted-path
                       ;; wording) and aborts the process — the only safe way
                       ;; to fail out of compiled code (no landing pads to
                       ;; unwind through across the JIT/AOT native-code
                       ;; boundary; the same rule `rt_match_fail` already
                       ;; follows). `Expr::Panic`'s own checked type is
                       ;; `Never`, so nothing downstream ever reads this call's
                       ;; return value for real.
                       (compile-panic ((builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Sexpr) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (e Sexpr)) llvm-value
                         (let ((msg-form (sexpr-car (sexpr-cdr e))))
                           (let ((msg-v (compile-value builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base msg-form)))
                             (let ((args-ptr (alloca-args builder 1)))
                               (store-arg builder args-ptr 0 msg-v)
                               (build-call builder (get-function m "rt_panic") args-ptr 1))))))
                (let ((v (compile-value builder env fn-env '() f (Option::none) (Option::none) (Option::none) body)))
                  (release-bindings builder m env param-names)
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
