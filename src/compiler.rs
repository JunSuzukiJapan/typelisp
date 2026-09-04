//! The (typelisp-hosted) compiler body: core IR (converted to the shape this
//! file reads by [`crate::compile::core_bridge`]) -> LLVM IR, built by calling the
//! `llvm-*` builtins (`crate::eval::interp`'s `eval_llvm_builtin_method`)
//! directly — the same "Rust provides the bindings, typelisp drives them"
//! split [docs/implementation-log.md](../docs/dev/implementation-log.md) calls for. Loaded the same way
//! `prelude.rs` loads the standard library: read -> check -> exec each
//! top-level form once, against the same `Heap`/`Checker`/`Interp` the rest
//! of the program uses.
//!
//! Compiles the node shapes `core_bridge::to_island` actually produces a
//! real translation for today — integer literals (`(int-any-width n)`), fixed-width
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
//! method-matching arm as later phases teach `core_bridge` to translate more
//! core IR forms for real.
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
//! any name `core_freevars::names_captured_by_nested` finds referenced inside a
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
//! matching the shared-cell semantics the interpreter gives a captured
//! binding through the same `BoxedObj::Cell` heap cells. The one
//! exception: a `labels` sibling captured *as a value* (not called) is
//! never cell-boxed even though it appears in the very same captured-list —
//! see `Ctx::visible_siblings`'s doc comment (`core_bridge.rs`) for why a
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
//! tag alongside it (`core_bridge`'s `name_kind_list`/`arg_pairs`, and
//! [`crate::check::repr::Repr::binding_kind`]'s doc comment) — `2` (`Sexpr`)
//! for every representation the collector can *reclaim* (that is every tagged
//! one except an interned symbol, which is tagged but immortal), `0` (plain)
//! for a raw scalar. This is what
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
//! `compile-value`/`compile-int-any-width`/`compile-var`/`compile-assoc`/`compile-apply`/
//! `compile-labels`/... are ordinary top-level `defun`s that call each other
//! freely. They used to be one ~2700-line `labels` block nested inside
//! `compile-function`, purely because a `defun` could not then reference one
//! defined below it: `compile-assoc` calling back into `compile-value` was a
//! forward reference, and CL's non-recursive `flet` does not help when the
//! calls genuinely are mutual recursion, so `labels` was the only tool that
//! worked. That restriction is gone — the ring is announced by the
//! `defsignature` block at the top of `SOURCE` — and it was flattened out
//! (docs/dev/two-pass-toplevel-plan.md, Phase 3).
//!
//! Flattening cost two explicit parameters. The `labels` block closed over
//! exactly two things from `compile-function`: `m`, the LLVM module, and the
//! name of the function being compiled (used by `declare-labels-siblings` to
//! build mangled sibling names). Both are now leading parameters on every one
//! of these functions — `m` first, then `fn-name`. The enclosing name is
//! spelled `fn-name` rather than `name` deliberately: several of these bind a
//! *local* `name` (`(let ((name (sexpr-str ...))) ...)`), which would shadow a
//! parameter of that spelling at exactly the call sites that must pass it on.
//!
//! Two consequences worth knowing:
//!
//! - `compile-function` is now a thin entry point — build the entry block,
//!   bind the parameters, call `compile-value`, emit the return.
//! - The island's call graph is no longer a DAG in declaration order, so
//!   `crate::compile::bootstrap` forward-declares every island function in the
//!   shared module *before* compiling any body. Without that, `compile-call`'s
//!   `(get-function m "tl_<callee>")` fails on the first call to a function
//!   declared further down.
//!
//! `builder`/`env`/`fn-env`/`captured` are likewise **explicit** parameters
//! rather than closed-over values (they have been since Phase 2):
//! `compile-labels` (below) needs to compile each
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
//! **What this SOURCE may name.** Builtins (`eq`/`append`/`car`/`cdr`/
//! `Scope`'s methods, `Option`'s `some`/`none`/pattern-matching), the
//! checker's native special forms (`if`/`let`/`match`/`labels`/`loop`/
//! `break`/`return`/`setf`), and the *core macro layer*
//! (`typelisp_front::core_macros` — `cond`/`case`/`when`/`unless`/`and`/`or`),
//! which is loaded before both this and the prelude for exactly that reason.
//!
//! Not prelude *functions*: a call to one is still there in the code this
//! island emits, and the island must not drag the prelude into it. A macro is
//! not a call — it is gone by the time anything is emitted — which is why the
//! core layer is fine here and why the island-local `icond`/`icase` copies it
//! used to carry are deleted. (This was once written as "no prelude names at
//! all"; the stricter reading cost the island `cond`/`case` for no benefit. An
//! island probe using the prelude's own `cond`/`case` bootstrapped cleanly,
//! since this SOURCE is checked with the prelude already in scope — the island
//! generator loads it for the *types* the signatures below use,
//! `Option<llvm-basic-block>` among them.)

use crate::{Checker, Heap, Interp};

pub const SOURCE: &str = r#"
;; ---------------------------------------------------------------------------
;; Forward declarations.
;;
;; This file is a recursive-descent compiler: `compile-value` and the ~50
;; helpers below it call each other in a ring, and a ring cannot be written
;; in definition order. Top-level `defun`s are checked one form at a time in
;; source order, so the way to say a name early is to say it here.
;;
;; Generated from the `defun` headers themselves; each must keep agreeing
;; with its definition, which the checker enforces at the definition.
(defsignature compile-apply
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-apply-indirect
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-assoc
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-assoc-user
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> string string Option<Sexpr>)
  llvm-value)
(defsignature compile-bignum-literal
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-call
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-call-args
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> llvm-value Option<Sexpr> i32)
  i32)
(defsignature compile-catch
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-cellset
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-construct
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-construct-box
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr> i32 Option<Sexpr>)
  llvm-value)
(defsignature compile-construct-boxed-struct
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr> Option<Sexpr>)
  llvm-value)
(defsignature compile-construct-boxed-struct-fields
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> llvm-value Option<Sexpr> i32)
  ())
(defsignature compile-construct-path
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-construct-path-segs
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> llvm-value Option<Sexpr> i32)
  ())
(defsignature compile-construct-sexpr
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> i32 Option<Sexpr>)
  llvm-value)
(defsignature compile-construct-sym
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-construct-sym-args
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> llvm-value Option<Sexpr> i32)
  ())
(defsignature compile-construct-wk-sym
  (llvm-module string llvm-builder Option<Sexpr>)
  llvm-value)
(defsignature compile-ctor-subpatterns
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> llvm-value i32 i32 Option<Sexpr> Option<Sexpr> i32 llvm-basic-block)
  ())
(defsignature compile-dyn-call
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-dyn-new
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-dyn-upcast
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-dyn-value
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-escaping-env-args
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-value Option<Sexpr> i32)
  ())
(defsignature compile-field-get
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-field-set
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-global-init
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-hashtable-op
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-if
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-if-branch
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> bool Option<Sexpr>)
  llvm-value)
(defsignature compile-labels
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-labels-bodies (llvm-module string Scope<llvm-function> Option<Sexpr> Option<Sexpr>) ())
(defsignature compile-lambda
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> Option<Sexpr>)
  llvm-value)
(defsignature compile-let
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-let-body
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-let-values
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr> Scope<llvm-value>)
  ())
(defsignature compile-llvm-op
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-loop
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-loop-body
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  ())
(defsignature compile-match
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-match-arms
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> bool llvm-value llvm-value llvm-basic-block Option<Sexpr>)
  ())
(defsignature compile-panic
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-pattern-test
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> llvm-value Option<Sexpr> llvm-basic-block)
  ())
(defsignature compile-ctor-pattern
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> llvm-value Option<Sexpr> llvm-basic-block)
  ())
(defsignature compile-ratio-literal
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-return
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-set
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-set-global
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-sexpr-instance-test
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> llvm-value Option<Sexpr> i32)
  llvm-value)
(defsignature compile-str
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-throw
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-unwind-protect
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-value
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature compile-vector-op
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> Option<Sexpr>)
  llvm-value)
(defsignature resolve-value
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> string)
  llvm-value)
(defsignature store-str-chars
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> llvm-function
   Option<llvm-basic-block> Option<llvm-value> Option<llvm-value> Option<llvm-basic-block>
   Option<llvm-basic-block> llvm-value Option<Sexpr> i32)
  ())
;;
;; Called from inside the ring but written after it.
(defsignature compile-bool (llvm-module string llvm-builder Option<Sexpr>) llvm-value)
(defsignature compile-break
  (llvm-module string llvm-builder Option<llvm-basic-block> Option<llvm-value> Option<llvm-value>
   Option<llvm-basic-block>)
  llvm-value)
(defsignature compile-cellvar
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> Option<Sexpr>)
  llvm-value)
(defsignature compile-char (llvm-module string llvm-builder Option<Sexpr>) llvm-value)
(defsignature compile-float-any-width (llvm-module string llvm-builder Option<Sexpr>) llvm-value)
(defsignature compile-global (llvm-module string llvm-builder Option<Sexpr>) llvm-value)
(defsignature compile-int-any-width (llvm-module string llvm-builder Option<Sexpr>) llvm-value)
(defsignature compile-unit (llvm-module string llvm-builder) llvm-value)
(defsignature compile-var
  (llvm-module string llvm-builder Scope<llvm-value> Scope<llvm-function> Option<Sexpr> Option<Sexpr>)
  llvm-value)
(defsignature emit-rt-call
  (llvm-builder llvm-module llvm-function string string llvm-value i32 Option<llvm-basic-block>)
  llvm-value)
;;
;; Not part of the ring — just written after its caller.
(defsignature emit-direct-call
  (llvm-builder llvm-module llvm-function llvm-function llvm-value i32 Option<llvm-basic-block>)
  llvm-value)


;; `sexpr-str`/`sexpr-bool`/`sexpr-sym-name`/`sexpr-i32` — the island's typed
;; `Sexpr` field extractors — are now Rust builtins (`Interp::eval_builtin`,
;; registered in `check::registry`), not `match`-based typelisp defuns.
;; Symbol/Sexpr redesign Phase 2 moved them out of this source so the island
;; navigates `Sexpr` structure entirely through the `sexpr-*` layer, never the
;; user-facing `match` (Phase 5 fences `match` to enum scrutinees). Each still
;; panics on a tag mismatch, exactly as the old `(_ (panic ...))` arms did.

;; Extracts Sexpr variant `variant`'s field `idx` (0-based) from tagged
;; value `v` -- the inverse of `typelisp-rt`'s `encode` for each field
;; kind representable in compiled code today: `i32`'s `i64` payload
;; (signed, `build-ashr`), `char`'s scalar (`build-lshr`), `bool`'s
;; payload (`1`/`2` -> `0`/`1`, matching `compile-bool`'s own convention),
;; `cons`'s two `Sexpr` fields (via `rt_car`/`rt_cdr` -- the only field kind
;; that needs a real heap read rather than pure bit manipulation), and (Stage
;; 7) `str`'s `Str` field. `str` is deliberately the *odd one out* among
;; these: unlike `i32`/`char`/`bool`, extraction here does *not* strip the
;; 3-bit tag -- it returns `v` unchanged. That's not an oversight: a bare
;; `Type::Str` value's compiled representation is *defined* to be the exact
;; same tagged immediate a `Sexpr::Str` already is (`typelisp-rt`'s
;; `rt_str_new` returns it pre-tagged for the same reason), specifically so
;; it stays heap-referencing-and-therefore-GC-root-eligible under
;; `rt_push_sexpr_root`/`rt_pop_sexpr_root`'s existing generic `decode` --
;; stripping the tag the way `char` does would make a `Str` field
;; indistinguishable from a plain integer to that machinery, silently
;; reopening the exact GC-safety gap Stage 4/6 closed for `Sexpr`. `f64`'s
;; `f64` field (Sexpr/RtValue unification, Stage 0) goes through
;; `rt_f64_value`/`rt_f32_value` -- a real heap read, like `cons`, since a float is now
;; boxed rather than an immediate bit pattern (see `typelisp-rt`'s
;; `TAG_BOXED`). `sym`'s `Symbol` field still isn't representable in
;; compiled code (a `Sym`'s tagged payload is the address of an interned
;; symbol's header; `Type::Symbol` has
;; no compiled representation, and `Interp::call_compiled`'s return-value
;; decode would degrade one to a plain `i32` at the JIT boundary), and
;; neither are `bignum`/`ratio`'s payloads (`Type::Bignum`/`Type::Ratio`
;; are heap objects with no `rt_bignum_*`/`rt_ratio_*` support yet — see
;; `core_bridge::bignum_form`), so a `Bind` pattern trying to
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
;; The `wsig` code (`width * 2 + signed`, `typelisp-rt`'s `wsig`) the five
;; narrow-integer `Sexpr` variants name: 12=i8 13=i16 14=u8 15=u16 16=u32.
;; A constant emitted from the *variant*, never read off the value — the
;; word a compiled `u8` sits in is the same bit pattern an `i32` would have,
;; which is the whole reason these are separate variants.
(defun narrow-wsig ((variant i32)) i32
  (case variant (12 17) (13 33) (14 16) (15 32) (16 64)
    (else (panic "narrow-wsig: not a narrow-integer Sexpr variant"))))

(defun compile-sexpr-field ((builder llvm-builder) (m llvm-module) (v llvm-value) (variant i32) (idx i32)) llvm-value
  (case variant
    ;; `i32`(1): the signed value sits above the 3 tag bits.
    (1 (build-ashr builder v (const-word builder 3)))
    ;; `f64`(2)/`f32`(11): the payload is inside the box, and which box it is
    ;; is what the two numbers say. `rt_f32_value` widens back into the `f64`
    ;; bit pattern a compiled float register holds -- the carrier is wider
    ;; than the type, the same way a `u8` rides in an `i64` word.
    (2 (let ((args-ptr (alloca-args builder 1)))
         (store-arg builder args-ptr 0 v)
         (build-call builder (get-function m "rt_f64_value") args-ptr 1)))
    (11 (let ((args-ptr (alloca-args builder 1)))
          (store-arg builder args-ptr 0 v)
          (build-call builder (get-function m "rt_f32_value") args-ptr 1)))
    ;; `char`(3) is unsigned; `bool`(4) undoes `compile-bool`'s `1`/`2`.
    (3 (build-lshr builder v (const-word builder 3)))
    (4 (build-sub builder (build-lshr builder v (const-word builder 3)) (const-word builder 1)))
    ;; `sym`(5)/`str`(6)/`bignum`(8)/`ratio`(9): the payload *is* the
    ;; already-tagged word — `Repr::field_kind`'s `6` passthrough — so there
    ;; is no bit manipulation and no box to open. (`5` reaches here from
    ;; `Sexpr` construction rather than from `field_kind`; see that
    ;; function's doc comment on the two producers of these numbers.)
    ((5 6 8 9) v)
    ;; `cons`(7): the only variant with two fields.
    (7 (let ((args-ptr (alloca-args builder 1)))
         (store-arg builder args-ptr 0 v)
         (if (eq idx 0)
             (build-call builder (get-function m "rt_car") args-ptr 1)
             (build-call builder (get-function m "rt_cdr") args-ptr 1))))
    ;; `path`(10): the payload isn't a scalar or an already-tagged value at
    ;; all — it's a fresh `Sexpr` list of `sym`s built from the tagged
    ;; `Value::Path`'s interned segments, mirroring `typelisp::eval::interp`'s
    ;; `match_sexpr_ctor` `SEXPR_PATH` arm exactly (`rt_path_to_list` does the
    ;; same rooted `cons`-chain build, just in Rust runtime code instead of
    ;; the self-hosted island).
    (10 (let ((args-ptr (alloca-args builder 1)))
          (store-arg builder args-ptr 0 v)
          (build-call builder (get-function m "rt_path_to_list") args-ptr 1)))
    ;; `i8`(12)/`i16`(13)/`u8`(14)/`u16`(15)/`u32`(16): the payload is inside
    ;; a `BoxedObj::Narrow`, and the variant says which type that box must
    ;; be. `rt_narrow_value` refuses any other, so a `(u8 x)` pattern can
    ;; never bind a `u16` node's value. What comes back is the plain
    ;; normalized word compiled code carries every integer in.
    ((12 13 14 15 16)
     (let ((args-ptr (alloca-args builder 2)))
       (store-arg builder args-ptr 0 v)
       (store-arg builder args-ptr 1 (const-word builder (narrow-wsig variant)))
       (build-call builder (get-function m "rt_narrow_value") args-ptr 2)))
    ;; `unit`(100): never a real `Sexpr` variant — this number only ever
    ;; arrives as a *struct/enum field* kind (`Repr::field_kind`), the decode
    ;; half of `compile-tag-struct-field`'s constant `6`. The stored word is
    ;; discarded for the same reason it was ignored on the way in, and the
    ;; result is the plain `0` every other `Unit`-typed value in compiled
    ;; code already is (`compile-unit`). It sat at `11` until `Sexpr` grew a
    ;; twelfth variant and wanted that number back.
    (100 (const-word builder 0))
    (else (panic "compile-sexpr-field: field type is not representable in compiled code yet"))))

;; The encode-side mirror of `compile-sexpr-field`'s decode, over the exact
;; same `Repr::field_kind`/`Sexpr`-variant numbering (`1`=i32
;; `2`=f64 `3`=char `4`=bool `6`=str/`Sexpr`/nested-boxed-struct/`Fn`
;; passthrough — a `defstruct`/`Vector<T>`/`cons-cell<K,V>`/closure-typed
;; field's value is already a properly tagged `Sexpr`, so it passes through
;; unchanged exactly like a `Str` — `0`=not representable yet: a
;; general-ADT/still-generic field) — `compile-construct-boxed-struct-fields`/
;; `compile-field-set` both call this to turn an already-compiled field
;; value `v` (a scalar kind's own untagged bit pattern, or an already-tagged
;; `Sexpr` for kind `6`) into the properly tagged `Sexpr` `rt_struct_new`/
;; `rt_struct_field_set` require (Stage 3 of the Sexpr/RtValue unification
;; plan, `docs/implementation-log.md`). Mirrors `compile-construct-sexpr`'s
;; own per-variant `i32`/`f64`/`char`/`bool`/`str` arms bit-for-bit rather
;; than calling into that function directly, since here `v` is already a
;; compiled value (there is no `arg-forms` sub-expression left to compile) --
;; see that function's own doc comment for why each shift/tag constant is
;; what it is.
(defun compile-tag-struct-field ((builder llvm-builder) (m llvm-module) (v llvm-value) (kind i32)) llvm-value
  (case kind
    ;; `i32`(1): the value moves above the 3 tag bits.
    (1 (build-shl builder v (const-word builder 3)))
    ;; `f64`(2)/`f32`(11): the two kinds that allocate, and they allocate
    ;; *different boxes*. Which one is not a detail the value can be asked
    ;; for at run time — the word is the same bit pattern either way — so it
    ;; has to come from the declared type, which is what this number is.
    (2 (let ((args-ptr (alloca-args builder 1)))
         (store-arg builder args-ptr 0 v)
         (build-call builder (get-function m "rt_f64_new") args-ptr 1)))
    (11 (let ((args-ptr (alloca-args builder 1)))
          (store-arg builder args-ptr 0 v)
          (build-call builder (get-function m "rt_f32_new") args-ptr 1)))
    (3 (build-or builder (build-shl builder v (const-word builder 3)) (const-word builder 4)))
    (4 (build-or builder (build-shl builder (build-add builder v (const-word builder 1)) (const-word builder 3)) (const-word builder 6)))
    ;; `6` is every already-tagged word: both directions leave it alone.
    (6 v)
    ;; `unit`(100): the slot holds a tagged `Value::Empty` -- `(IMMEDIATE_NIL
    ;; << 3) | TAG_IMMEDIATE`, i.e. the constant `6` (`typelisp-rt`'s
    ;; `encode`), the same word an interpreted writer puts in a `()` field,
    ;; so the two produce identical boxes. `v` (the plain `0` `compile-unit`
    ;; produced) is deliberately discarded: a unit type has one value, so the
    ;; slot carries no information and only has to hold a word the GC can
    ;; `decode` safely -- an immediate nil references nothing.
    ;; `compile-sexpr-field` above is the inverse.
    (100 (const-word builder 6))
    (else (panic "compile-tag-struct-field: field type is not representable in compiled code yet"))))

;; `names` is now a list of `(name . kind)` pairs (`core_bridge::name_kind_list`
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
;; `core_bridge::name_kind_list`'s doc comment for the `10 + struct_field_kind`
;; numbering): this parameter is captured by some closure nested in its own
;; function body, so it must be a shared `BoxedObj::Cell` (`rt_cell_new`), not
;; a plain stack slot — a `setf` inside the capturing closure has to be
;; visible here, and vice versa. The incoming raw argument word is first
;; tagged per `kind - 10` (`compile-tag-struct-field`, the same encode a
;; struct field's own write already uses) since `rt_cell_new` expects an
;; already-tagged `Sexpr`; the tagged value is transiently rooted around that
;; one call (`push-sexpr-root`/`pop-sexpr-root`) since tagging a `kind = 2`
;; (`f64`) payload allocates a fresh `rt_f64_new` box that would otherwise
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
(defun bind-params ((env Scope<llvm-value>) (builder llvm-builder) (m llvm-module) (f llvm-function) (names Option<Sexpr>) (idx i32)) ()
  (if (sexpr-consp names)
      (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
        (let ((nm (sexpr-sym-name (sexpr-car name-pair))) (kind (sexpr-i32 (sexpr-cdr name-pair))))
          (let* ((raw (load-arg builder f idx))
                 (slot (alloca-args builder 2)))
            (if (>= kind 10)
                (let* ((tagged (compile-tag-struct-field builder m raw (- kind 10)))
                       (tag-args-ptr (alloca-args builder 1)))
                  (store-arg builder tag-args-ptr 0 tagged)
                  (let* ((ignored1 (build-call builder (get-function m "rt_push_sexpr_root") tag-args-ptr 1))
                         (cell-args-ptr (alloca-args builder 1)))
                    (store-arg builder cell-args-ptr 0 tagged)
                    (let* ((cell-ref (build-call builder (get-function m "rt_cell_new") cell-args-ptr 1))
                           (ignored2 (build-call builder (get-function m "rt_pop_sexpr_root") (alloca-args builder 0) 0))
                           (root-args-ptr (alloca-args builder 1)))
                      (store-arg builder root-args-ptr 0 cell-ref)
                      (let ((ignored3 (build-call builder (get-function m "rt_push_sexpr_root") root-args-ptr 1)))
                        (store-arg builder slot 0 cell-ref)))))
                (store-arg builder slot 0 raw))
            (set env nm slot))
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
;; now (closure-representation unification, Stage 4 — `core_bridge`'s shared
;; captured-list builder `captured_with_reprs` tags *every* capture as cell-boxed,
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
(defun bind-captures ((env Scope<llvm-value>) (builder llvm-builder) (m llvm-module) (f llvm-function) (names Option<Sexpr>) (idx i32)) ()
  (if (sexpr-consp names)
      (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
        (let ((nm (sexpr-sym-name (sexpr-car name-pair))) (kind (sexpr-i32 (sexpr-cdr name-pair))))
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

;; 2^n by repeated doubling, as an `i32` *bit pattern* — `compute-sexpr-mask-part`'s
;; one helper, not a generic utility; `n` (a captured-slot index) is always
;; small in practice so the O(n) recursion is in no way a bottleneck.
;;
;; `n = 31` doubles into the sign bit and yields `i32`'s minimum, which is the
;; right pattern for bit 31 and the reason this is not written as a number.
;; Callers keep `n` inside `0..32` themselves (`compute-sexpr-mask-part`), so
;; the wrap-to-zero past that is never reached.
(defun pow2 ((n i32)) i32
  (if (eq n 0) 1 (* (pow2 (- n 1)) 2)))

;; One 32-bit half of `build-make-closure`'s `sexpr_mask` argument (formerly
;; `fn_mask`): slots `base .. base+32` of a bitmask, one bit per captured slot,
;; set wherever that
;; slot's `kind` tag (`Repr::binding_kind` — `0`=plain, `2`=sexpr;
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
;;
;; Two halves rather than one word because the mask has one bit per slot and
;; the language's widest fixed-width integer is `i32`: `build-make-closure`
;; takes `base = 0` and `base = 32` and puts them back together on the Rust
;; side, so a closure may still capture the 64 slots `rt_closure_new`'s own
;; `u64` mask can describe.
(defun compute-sexpr-mask-part ((names Option<Sexpr>) (idx i32) (base i32)) i32
  (if (sexpr-consp names)
      (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
        (let ((kind (sexpr-i32 (sexpr-cdr name-pair))))
          ;; Trace a slot whose tag is `2` (a plain tagged `Sexpr` value) *or*
          ;; `>= 10` (a cell-boxed capture — `tagged_sym_list`'s `10 +
          ;; struct_field_kind`): the cell slot holds a `BoxedObj::Cell`
          ;; reference, itself a live heap value the GC mark phase must follow
          ;; to keep the captured binding (and whatever it holds) alive. Missing
          ;; the `>= 10` case let a nested closure's cell-boxed capture — e.g. a
          ;; compiled function's own parameter captured by an inner `lambda` —
          ;; be swept out from under it under GC pressure.
          (if (if (if (eq kind 2) true (>= kind 10))
                  (if (>= idx base) (< idx (+ base 32)) false)
                  false)
              (+ (pow2 (- idx base)) (compute-sexpr-mask-part rest (+ idx 1) base))
              (compute-sexpr-mask-part rest (+ idx 1) base))))
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
;;
;; The `sexpr-symp` guard this used to open with is gone: comparing against
;; the interned `var` symbol is already a type test — a cons or a literal is
;; simply not that symbol — where reading a name out of the head first was
;; not defined unless the head was one.
(defun form-is-borrowed? ((env Scope<llvm-value>) (form Option<Sexpr>)) bool
  (if (equal (sexpr-car form) (quote var))
      (name-is-borrowed? env (sexpr-str (sexpr-car (sexpr-cdr form))))
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
(defun retain-bindings ((builder llvm-builder) (m llvm-module) (env Scope<llvm-value>) (names Option<Sexpr>)) ()
  (if (sexpr-consp names)
      (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
       (let* ((nm (sexpr-sym-name (sexpr-car name-pair)))
              (kind (sexpr-i32 (sexpr-cdr name-pair))))
         (if (eq kind 2)
           (match (get env nm)
             ((Some slot)
              (let* ((v (load-raw builder slot 0))
                     (root-idx (build-call builder (get-function m "rt_root_count") (alloca-args builder 0) 0)))
                (store-arg builder slot 1 root-idx)
                (let ((args-ptr (alloca-args builder 1)))
                  (store-arg builder args-ptr 0 v)
                  (let ((ignored (build-call builder (get-function m "rt_push_sexpr_root") args-ptr 1))) ()))))
             (None ()))
           ())
       (retain-bindings builder m env rest)))
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
(defun release-bindings ((builder llvm-builder) (m llvm-module) (env Scope<llvm-value>) (names Option<Sexpr>)) ()
  (if (sexpr-consp names)
      (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
       (let ((kind (sexpr-i32 (sexpr-cdr name-pair))))
         (if (if (eq kind 2) true (>= kind 10))
             (let* ((args-ptr (alloca-args builder 0))
                    (ignored (build-call builder (get-function m "rt_pop_sexpr_root") args-ptr 0)))
               ())
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
  (let* ((args-ptr (alloca-args builder 0))
         (ignored (build-call builder (get-function m "rt_pop_sexpr_root") args-ptr 0)))
    ()))

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

;; Shared two-operand `rt_int_*` call shape (`ash`/`logbitp`, whose shift/
;; index argument makes a bare LLVM instruction unsafe — see
;; `int-native-method?`'s doc comment) — the integer counterpart of
;; `bignum-binop-call`. `wsig` is the receiver type's width and signedness
;; (`int-wsig`), the third operand every integer shim takes.
(defun int-binop-shim-call ((builder llvm-builder) (m llvm-module) (fname string) (x llvm-value) (y llvm-value) (wsig i32)) llvm-value
  (let ((args-ptr (alloca-args builder 3)))
    (store-arg builder args-ptr 0 x)
    (store-arg builder args-ptr 1 y)
    (store-arg builder args-ptr 2 (const-word builder wsig))
    (build-call builder (get-function m fname) args-ptr 3)))

;; Shared one-operand `rt_int_*` call shape (`logcount`/`integer-length`) —
;; the integer counterpart of `bignum-unary-call`, `wsig` as above.
(defun int-unary-shim-call ((builder llvm-builder) (m llvm-module) (fname string) (x llvm-value) (wsig i32)) llvm-value
  (let ((args-ptr (alloca-args builder 2)))
    (store-arg builder args-ptr 0 x)
    (store-arg builder args-ptr 1 (const-word builder wsig))
    (build-call builder (get-function m fname) args-ptr 2)))

;; One raw operand in, one out — the `rt_f64_*` transcendentals that have no
;; LLVM intrinsic in the pinned version. These used to share
;; `int-unary-shim-call`, which stopped fitting when the integer shims grew
;; their `wsig` operand; they never had anything to do with integers anyway.
(defun unary-shim-call ((builder llvm-builder) (m llvm-module) (fname string) (x llvm-value)) llvm-value
  (let ((args-ptr (alloca-args builder 1)))
    (store-arg builder args-ptr 0 x)
    (build-call builder (get-function m fname) args-ptr 1)))

;; `compile-assoc`'s native-method dispatch predicates: is `method` one of
;; the receiver-type methods that lower to LLVM instructions (i64/i32) or
;; `rt_str_*` primitive calls (string) rather than a real function call?
;; Anything *not* on these lists — a user-defined method on a primitive
;; receiver, e.g. the prelude's `impl Eq i32` → `equals` — falls through to
;; `compile-assoc-user`'s ordinary mangled-name call. (Chained `if`s: the
;; prelude's `or` macro isn't loaded under `run_with_compiler`.)
;; Which receiver type names go to `int-native-method?`/`float-native-method?`
;; above. Every integer width is one catalog and one 64-bit register, and both
;; float widths one 64-bit word — but the width is *not* only a type-checking
;; distinction: it decides where each arithmetic result is cut back to
;; (`build-normalize-int`) and what the `rt_int_*` shims are told, so the
;; lowering that serves `i32` serves `u8` only up to that one constant. Kept
;; as their own predicates rather than inlined at the dispatch so the two
;; lists read next to each other.
(defun int-receiver-type? ((name string)) bool
  (case name
    (("i8" "i16" "i32" "u8" "u16" "u32") true)
    (else false)))

(defun float-receiver-type? ((name string)) bool
  (case name
    (("f32" "f64") true)
    (else false)))

;; The receiver type's width and signedness as one number,
;; `width * 2 + (signed ? 1 : 0)` — `types::int_width_signed` on the Rust
;; side, and the third operand every `rt_int_*` shim takes. A type name says
;; how many bits and whether the top one is a sign, and nothing else; this is
;; where the island reads both off.
(defun int-wsig ((name string)) i32
  (case name
    ("i8" 17) ("i16" 33) ("i32" 65)
    ("u8" 16) ("u16" 32) ("u32" 64)
    (else (panic (append "int-wsig: not an integer type: " name)))))

;; `v` cut back to the type's width and re-extended into the 64-bit register
;; every compiled integer lives in: `shl` then `ashr` (signed) or `lshr`
;; (unsigned) — `types::normalize_int`'s IR.
;;
;; A shift pair rather than a mask because the mask for a 32-bit width is
;; itself past `i32`, a constant this island cannot write. Emitted after
;; `+`/`-`/`*` and `lognot`, the operations whose 64-bit result can carry bits
;; the type does not have; the comparisons, `/`, `mod` and `and`/`or`/`xor`
;; need none, because operands arrive normalized and those preserve it.
(defun build-normalize-int ((builder llvm-builder) (v llvm-value) (wsig i32)) llvm-value
  (let ((shift (const-word builder (- 64 (/ wsig 2)))))
    (if (eq (mod wsig 2) 1)
        (build-ashr builder (build-shl builder v shift) shift)
        (build-lshr builder (build-shl builder v shift) shift))))

(defun int-native-method? ((method string)) bool
  (case method
    (("+" "-" "*") true)
    ;; `/`/`mod` lower to the `rt_int_div`/`rt_int_mod` shims (integer division
    ;; can't be a bare LLVM instruction — `sdiv`/`srem` by zero is UB), not a
    ;; straight instruction like the others, but they're still native here.
    (("/" "mod") true)
    (("<" "<=" ">" ">=") true)
    ;; `eq`/`eql`/`equal`/`equalp` are four more names for `=` on an integer
    ;; (same-type operands, no case to fold and no structure to recurse
    ;; into), so all five are the same `icmp eq`. `case` expands to `equal`
    ;; comparisons, so leaving these out made every `case` over an integer
    ;; uncompilable.
    (("=" "eq" "eql" "equal" "equalp" "/=") true)
    ;; `int->bignum`/`int->ratio`: always-exact widening into the two
    ;; arbitrary-precision types (`rt_int_to_bignum`/`rt_int_to_ratio`) — unary,
    ;; checked before `b2` the same way `float-native-method?`'s own unary
    ;; conversions are. `int->float` is the third of them and a single
    ;; `sitofp`, the inverse of `float->int`'s `fptosi`.
    (("int->bignum" "int->ratio" "int->float") true)
    ;; `int->char`: the identity on the word, plus the Unicode-scalar-value
    ;; check that makes it a `char`. The check is the whole reason it is a
    ;; shim (`rt_int_to_char`) while its inverse `char->int` is nothing at
    ;; all, and the reason it goes through `emit-direct-call`: it can raise.
    ("int->char" true)
    ;; The `Option`-returning conversions. No prelude definition reaches
    ;; these; they are lowered so a *user's* `defun` naming one — which is
    ;; what `(try-as char n)` expands to — can be compiled at all.
    ("try-int->char" true)
    (("try-int->i8" "try-int->i16" "try-int->i32" "try-int->u8" "try-int->u16" "try-int->u32") true)
    ;; `max`/`min`: `icmp`+`select`, branch-free (`build-select`).
    (("max" "min") true)
    ;; `logand`/`logior`/`logxor`: bare LLVM instructions
    ;; (`build-and`/`build-or`/`build-xor`), same as `+`/`-`/`*`. `lognot` is
    ;; one too — `xor -1` — which is why it sits here rather than with the
    ;; shims below, whatever the old chain's ordering suggested.
    (("logand" "logior" "logxor" "lognot") true)
    ;; `logtest`: `icmp ne (and a b), 0` — an `and` plus the existing
    ;; `build-icmp-ne`, no shim needed.
    ("logtest" true)
    ;; `ash`/`logbitp`/`logcount`/`integer-length`: each has an edge case a
    ;; bare LLVM instruction can't express safely (a variable shift/count past
    ;; the operand's bit width is undefined behavior in LLVM, unlike this
    ;; language's own clamped semantics — see `eval_int_builtin`'s doc
    ;; comment), so all four lower to `rt_int_*` shims instead, the same
    ;; reasoning `/`/`mod` already use.
    (("ash" "logbitp" "logcount" "integer-length") true)
    ;; `int->i8`/`int->u32`/...: a cast between two integer widths, which is
    ;; `build-normalize-int` at the *target*'s width — the same two shifts
    ;; every arithmetic result already ends with. The `try-` counterparts are
    ;; not here: they build an `Option`, like `try-int->char`, and share its
    ;; gap.
    (("int->i8" "int->i16" "int->i32" "int->u8" "int->u16" "int->u32") true)
    (else false)))

;; `sexpr`'s natively-compilable methods (`registry::sexpr_assoc` registers
;; exactly these two). `eq` is an `icmp eq` on the tagged handle; `eql` is the
;; `rt_sexpr_eql` shim, since it has to see inside float/narrow-integer/
;; `bignum`/`ratio`
;; boxes. The structural `equal`/`equalp` are free *functions*, not methods,
;; so they reach their own shims through `rt_builtin_symbol` instead of here.
(defun sexpr-native-method? ((method string)) bool
  (case method
    (("eq" "eql") true)
    (else false)))

(defun string-native-method? ((method string)) bool
  (case method
    (("length" "ref" "eq" "eql" "equal" "equalp" "lt" "<" "<=" ">" ">="
      "append") true)
    ;; `substring` is the one three-operand string method
    ;; (`rt_str_substring`); every other one here is unary or binary.
    ("substring" true)
    (("upcase" "downcase") true)
    (else false)))

;; The string methods that take the receiver and nothing else, and so must be
;; recognized before `compile-assoc`'s string branch reads a second operand.
;; `length` is not here: it has a differently-shaped result (a bare count, not
;; a string) and its own arm already.
(defun string-unary-shim-method? ((method string)) bool
  (case method
    (("upcase" "downcase") true)
    (else false)))

;; The `rt_str_*` shim behind each of those — the string counterpart of
;; [`char-unary-shim-name`].
(defun string-unary-shim-name ((method string)) string
  (case method
    ("upcase" "rt_str_upcase")
    ("downcase" "rt_str_downcase")
    (else (panic (append "string-unary-shim-name: not a string shim method " method)))))

;; `char`'s natively-compilable methods: a compiled `char` is a raw `i64`
;; code point, so the content comparisons lower to the same integer `icmp`s
;; the int branch uses. `equalp` (ASCII case-insensitive) has no single
;; instruction and stays non-native. `char->int` is the identity at the
;; compiled level (a `char`'s value *is* its code point, and it fits the
;; carrier unchanged) — needed so the island's own `compile-char` (which calls
;; `(char->int (sexpr-char ...))`) is itself compilable.
;; `char->string` is a one-character `rt_str_new` call, since that shim's
;; arguments are exactly raw code points — the single most expensive gap the
;; precompiled prelude found (31 definitions, the string scanners and the
;; reader among them, reach it).
(defun char-native-method? ((method string)) bool
  (case method
    (("eq" "eql" "equal" "equalp" "lt" "<" "<=" ">" ">=") true)
    ;; `char->int` is the identity at the compiled level and `char->string`
    ;; is a one-character `rt_str_new`; both are unary, which is why
    ;; `compile-assoc` checks them before reading a second operand.
    (("char->int" "char->string") true)
    ;; `upcase`/`downcase`/`alphap`/`digitp`: one `rt_char_*` call each, all
    ;; unary for the same reason. The rule they apply is ASCII-only, and it
    ;; is the shim's rule rather than one written here — so the compiled
    ;; answer cannot drift from the interpreted one.
    (("upcase" "downcase" "alphap" "digitp") true)
    (else false)))

;; The four `char` methods that are a shim call rather than an instruction,
;; and the shim each one names. Split from `char-native-method?` because the
;; lowering needs the *name*, and a predicate that answered `true` would leave
;; `compile-assoc` re-deriving it.
(defun char-unary-shim-method? ((method string)) bool
  (case method
    (("upcase" "downcase" "alphap" "digitp") true)
    (else false)))

(defun char-unary-shim-name ((method string)) string
  (case method
    ("upcase" "rt_char_upcase")
    ("downcase" "rt_char_downcase")
    ("alphap" "rt_char_alphap")
    ("digitp" "rt_char_digitp")
    (else (panic (append "char-unary-shim-name: not a char shim method " method)))))

;; `bool`'s natively-compilable methods. `registry::bool_assoc` registers
;; `eq`/`eql`/`equal`/`equalp` as four names for one operation (two immediate
;; values, no case folding and no structure to recurse into), and a compiled
;; `bool` is a raw `0`/`1`, so all four are the same `icmp eq` — the same
;; shape the `char` comparisons take.
(defun bool-native-method? ((method string)) bool
  (case method
    (("eq" "eql" "equal" "equalp") true)
    (else false)))

;; `symbol`'s natively-compilable methods: `eq`/`eql`, the only two
;; `registry::symbol_assoc` registers. A compiled `symbol` is its interned
;; handle, so identity is handle equality — the `sexpr` arm's `eq` again,
;; with a receiver the checker spells `symbol` rather than `sexpr`.
(defun symbol-native-method? ((method string)) bool
  (case method
         ("eq" true)
         (else (equal method "eql"))))

;; `f64`'s natively-compilable methods: arithmetic (`+`/`-`/`*`/`/`) lowers to
;; LLVM float instructions (`build-fadd`/... — each `bitcast`s the
;; `i64`-carried `f64` bits to `double` and back internally), comparisons
;; (`<`/`<=`/`>`/`>=`/`=`/`/=` and the `eq`/`eql`/`equal`/`equalp` aliases) to
;; `build-fcmp-*`. `expt` and the unary `sqrt`/`floor`/`ceiling`/`round`/
;; `truncate` family lower to the corresponding LLVM intrinsic (`build-fpow`/
;; `build-fsqrt`/...), and `float->int` to a single `fptosi` instruction
;; (`build-fptosi`) — all native, same as the arithmetic ops. `float->bignum`/
;; `float->ratio` are unary too (`rt_float_to_bignum`/`rt_float_to_ratio`),
;; now that `bignum`/`ratio` have a compiled representation.
;;
;; `mod`/`rem` are deliberately NOT native: CL `mod` is floored and `rem`
;; truncated, so they're ordinary `prelude.rs` methods (`a - b*floor|trunc(a/b)`)
;; compiled the normal way. (The `build-frem` arm in the dispatch below is now
;; unreachable for `mod` and left only as a no-op.)
(defun float-native-method? ((method string)) bool
  (case method
    (("+" "-" "*" "/" "expt" "sqrt" "floor" "ceiling" "round" "truncate"
      "float->int" "float->bignum" "float->ratio" "<" "<=" ">" ">=" "=" "/="
      "eq" "eql" "equal" "equalp") true)
    ;; `float->f32`/`float->f64`: the cast between the two float widths.
    ;; Narrowing is `build-fround32`, widening is nothing at all — a value in
    ;; the register is already the binary32 one. `try-float->f32` is not here:
    ;; it builds an `Option`, and shares `try-int->char`'s gap.
    (("float->f32" "float->f64") true)
    (("try-float->f32" "try-float->f64") true)
    ;; `max`/`min`: `llvm.maxnum.f64`/`llvm.minnum.f64` (`build-fmaxnum`/
    ;; `build-fminnum`), same intrinsic-call shape as `expt`'s `llvm.pow.f64`.
    (("max" "min") true)
    ;; The transcendental family: `sin`/`cos`/`exp`/`log` have long-standing
    ;; LLVM intrinsics (`build-fsin`/`build-fcos`/`build-fexp`/`build-flog`,
    ;; same shape as `sqrt`). `tan`/`asin`/`acos`/`atan`/`sinh`/`cosh`/`tanh`/
    ;; `asinh`/`acosh`/`atanh` have none in the LLVM version this project pins,
    ;; so they lower to `rt_f64_*` shims instead — still native here, same
    ;; reasoning `/`/`mod` already use for `i64`.
    (("sin" "cos" "tan" "asin" "acos" "atan" "sinh" "cosh" "tanh" "asinh"
      "acosh" "atanh" "exp" "log") true)
    (else false)))

;; Whether `method`'s *result* is a float, so an `f32` receiver has to round
;; it. The conversions out of the float family (`float->int`/`->bignum`/
;; `->ratio`) and the comparisons are the ones that are not, and
;; `float->f32`/`float->f64` are excluded because they name their own target
;; — rounding those by the *receiver*'s width would be the wrong width.
(defun float-result-method? ((method string)) bool
  (case method
    (("+" "-" "*" "/" "mod" "expt" "sqrt" "floor" "ceiling" "round" "truncate"
      "max" "min" "sin" "cos" "tan" "asin" "acos" "atan" "sinh" "cosh" "tanh"
      "asinh" "acosh" "atanh" "exp" "log") true)
    (else false)))

;; `v` rounded to binary32 when the receiver is `f32` and the method's result
;; is a float — what makes `f32` arithmetic binary32 arithmetic rather than
;; `f64` arithmetic under another name (`docs/functions.md` §1b). Every other
;; case returns `v` untouched.
(defun round-f32-if ((builder llvm-builder) (type-name string) (method string) (v llvm-value)) llvm-value
  (if (if (equal type-name "f32") (float-result-method? method) false)
      (build-fround32 builder v)
      v))

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
  (case method
    (("+" "-" "*" "/" "mod" "<" "<=" ">" ">=" "=" "/=" "eq" "eql" "equal"
      "equalp" "bignum->int" "try-bignum->int" "bignum->float" "bignum->ratio") true)
    ;; `max`/`min`: `rt_bignum_cmp` (already used by every comparison above)
    ;; plus `build-select`, branch-free — no new runtime helper needed.
    (("max" "min") true)
    ;; `logand`/`logior`/`logxor`/`lognot`: `rt_bignum_*` calls, not the
    ;; bare LLVM instructions `i64` gets — the operands are boxed
    ;; arbitrary-precision values. These four are what the prelude's
    ;; derived bitwise operators (`logeqv`/`lognand`/`lognor`/`logandc1`/
    ;; `logandc2`/`logorc1`/`logorc2`) are written in terms of.
    (("logand" "logior" "logxor" "lognot") true)
    ;; `ash` joined them when `Bits`'s `shift` impl for `bignum` gave it a
    ;; prelude caller. Its second operand is a raw bit count rather than a
    ;; second boxed value, which `bignum-binop-call` does not care about (it
    ;; stores two words) but `rt_bignum_ash` does.
    (("ash") true)
    ;; The rest of `bignum_assoc`'s bitwise catalog. No prelude definition
    ;; reaches these — they are here so that a *user's* `defun` naming one
    ;; can still be compiled, which is the whole content of the "builtins
    ;; with no compiled implementation" list in `docs/syntax.md` §10.
    ;; `logbitp`'s second operand is a raw position like `ash`'s count;
    ;; `logbitp`/`logtest` return a bare `0`/`1` (a compiled `bool`), and
    ;; `logcount`/`integer-length` return fresh boxed bignums.
    (("logbitp" "logtest" "logcount" "integer-length") true)
    (else false)))

;; `ratio` (`registry::ratio_assoc`)'s natively-compilable methods — the
;; `ratio` counterpart of [`bignum-native-method?`] (no `mod`, CL doesn't
;; define a rational remainder), plus `ratio->bignum`/`ratio->float`/
;; `numerator`/`denominator`.
(defun ratio-native-method? ((method string)) bool
  (case method
    (("+" "-" "*" "/" "<" "<=" ">" ">=" "=" "/=" "eq" "eql" "equal" "equalp"
      "ratio->bignum" "ratio->float" "numerator" "denominator") true)
    ;; `max`/`min`: `rt_ratio_cmp` + `build-select`, same shape as `bignum`'s.
    (("max" "min") true)
    (else false)))

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

;; [`bignum-binop-call`]'s shape for the shims that can *raise*: a zero
;; divisor (`rt_int_div`/`rt_int_mod`/`rt_bignum_div`/`rt_bignum_mod`/
;; `rt_ratio_div`), an index past the end of a string (`rt_str_ref`). Those
;; unwind now (`typelisp_rt::raise`) instead of aborting the process, and an
;; unwind is only caught by the frame that *made* the call — so this goes
;; through `emit-direct-call`, which inside a `catch`/`unwind-protect` region
;; hands the target to `rt_protected_call` rather than calling it here. A
;; plain `build-call` would fly past the enclosing region's own cleanups.
(defun raising-binop-call ((builder llvm-builder) (m llvm-module) (cur-fn llvm-function) (protect Option<llvm-basic-block>) (fname string) (x llvm-value) (y llvm-value)) llvm-value
  (let ((args-ptr (alloca-args builder 2)))
    (store-arg builder args-ptr 0 x)
    (store-arg builder args-ptr 1 y)
    (emit-direct-call builder m cur-fn (get-function m fname) args-ptr 2 protect)))

;; [`raising-binop-call`] with the integer shims' third `wsig` operand —
;; `rt_int_div`/`rt_int_mod`, the two that both raise *and* need to know the
;; receiver's width.
(defun raising-int-binop-call ((builder llvm-builder) (m llvm-module) (cur-fn llvm-function) (protect Option<llvm-basic-block>) (fname string) (x llvm-value) (y llvm-value) (wsig i32)) llvm-value
  (let ((args-ptr (alloca-args builder 3)))
    (store-arg builder args-ptr 0 x)
    (store-arg builder args-ptr 1 y)
    (store-arg builder args-ptr 2 (const-word builder wsig))
    (emit-direct-call builder m cur-fn (get-function m fname) args-ptr 3 protect)))

;; Counts a plain `Sexpr` list's elements — used to size the `i64*` args
;; array a direct call needs (`compile-apply`'s `alloca-args`/`build-call`),
;; and (labels/closures Stage 2) the `i64*` env array a captured call needs.
(defun sexpr-list-length ((s Option<Sexpr>)) i32
  (if (sexpr-consp s)
      (+ 1 (sexpr-list-length (sexpr-cdr s)))
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
(defun bind-let-values ((builder llvm-builder) (m llvm-module) (env Scope<llvm-value>) (bindings Option<Sexpr>) (acc Scope<llvm-value>)) ()
  (if (sexpr-consp bindings)
      (let ((pair (sexpr-car bindings)) (rest (sexpr-cdr bindings)))
       (let* ((name-pair (sexpr-car pair))
              (nm (sexpr-sym-name (sexpr-car name-pair)))
              (kind (sexpr-i32 (sexpr-cdr name-pair))))
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
       (bind-let-values builder m env rest acc)))
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
(defun unroot-let-sexpr-values ((builder llvm-builder) (m llvm-module) (bindings Option<Sexpr>)) ()
  (if (sexpr-consp bindings)
      (let ((pair (sexpr-car bindings)) (rest (sexpr-cdr bindings)))
       (let ((kind (sexpr-i32 (sexpr-cdr (sexpr-car pair)))))
         (if (if (eq kind 2) true (>= kind 10))
             (let* ((args-ptr (alloca-args builder 0))
                    (ignored (build-call builder (get-function m "rt_pop_sexpr_root") args-ptr 0)))
               ())
             ())
         (unroot-let-sexpr-values builder m rest)))
      ()))

;; Tests whether tagged Sexpr value `v` is Sexpr variant `variant`
;; (`registry::sexpr_def`'s variant order: 0=nil 1=i32 2=f64 3=char
;; 4=bool 5=sym 6=str 7=cons 8=bignum 9=ratio 10=path 11=f32 12=i8 13=i16
;; 14=u8 15=u16 16=u32) -- `nil`/`bool` both compile
;; to the same 3-bit tag (`typelisp-rt`'s `TAG_IMMEDIATE`, 6) and are told
;; apart by `v`'s payload bits instead (`0` vs non-zero, see that crate's
;; `encode`/`decode`). The two float widths, the five narrow integer widths,
;; `bignum` and `ratio` all share `TAG_BOXED` (7)
;; with every other heap-boxed object, so the tag alone can't tell them
;; apart -- they go through `rt_box_kind` (a match already
;; implies the tag is `TAG_BOXED`, so no separate tag check is emitted).
;; Only `i32` reads its value straight out of the tagged word: that is the
;; width a bare `Sexpr` integer means, and the other five say which they are
;; from inside their box.
;; Every remaining variant has its own dedicated tag.
;; Whether `v`'s low three bits are `tag` — the shared half of every
;; non-boxed `Sexpr` tag test below.
(defun compile-tag-bits-test ((builder llvm-builder) (v llvm-value) (tag i32)) llvm-value
  (build-icmp-eq builder (build-and builder v (const-word builder 7)) (const-word builder tag)))

;; Whether `v` is a heap box of kind `kind` (`rt_box_kind`'s numbering:
;; 1 = f64, 2 = bignum, 3 = ratio, 4 = f32, 5/6/7/8/9 = i8/i16/u8/u16/u32) —
;; the nine `Sexpr` variants that a tag test cannot tell apart, since they
;; all carry `TAG_BOXED`.
(defun compile-box-kind-test ((builder llvm-builder) (m llvm-module) (v llvm-value) (kind i32)) llvm-value
  (let ((args-ptr (alloca-args builder 1)))
    (store-arg builder args-ptr 0 v)
    (build-icmp-eq builder
                   (build-call builder (get-function m "rt_box_kind") args-ptr 1)
                   (const-word builder kind))))

(defun compile-sexpr-tag-test ((builder llvm-builder) (m llvm-module) (v llvm-value) (variant i32)) llvm-value
  (case variant
    ;; `f64`(2)/`bignum`(8)/`ratio`(9)/`f32`(11) and the five narrow integer
    ;; widths(12..16) are heap boxes that share one tag, so what tells them
    ;; apart is the box's own kind, not the tag bits.
    (2 (compile-box-kind-test builder m v 1))
    (8 (compile-box-kind-test builder m v 2))
    (9 (compile-box-kind-test builder m v 3))
    (11 (compile-box-kind-test builder m v 4))
    (12 (compile-box-kind-test builder m v 5))
    (13 (compile-box-kind-test builder m v 6))
    (14 (compile-box-kind-test builder m v 7))
    (15 (compile-box-kind-test builder m v 8))
    (16 (compile-box-kind-test builder m v 9))
    ;; `nil`(0) and `bool`(4) share the immediate tag `6` and differ only in
    ;; the payload: nil's is zero, a bool's never is.
    (0 (build-and builder
                  (compile-tag-bits-test builder v 6)
                  (build-icmp-eq builder (build-lshr builder v (const-word builder 3)) (const-word builder 0))))
    (4 (build-and builder
                  (compile-tag-bits-test builder v 6)
                  (build-icmp-ne builder (build-lshr builder v (const-word builder 3)) (const-word builder 0))))
    ;; Everything else is one comparison against `typelisp-abi`'s tag table.
    (else (compile-tag-bits-test builder v
                                 (case variant
                                   (1 0) (3 4) (5 2) (6 3) (7 1) (10 5)
                                   (else (panic "compile-sexpr-tag-test: unknown Sexpr variant")))))))

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
(defun compile-box-tag-test ((builder llvm-builder) (m llvm-module) (v llvm-value) (variant i32)) llvm-value
  (let ((args-ptr (alloca-args builder 1)))
    (store-arg builder args-ptr 0 v)
    (build-icmp-eq builder (build-call builder (get-function m "rt_data_variant") args-ptr 1) (const-word builder variant))))

;; The box-scrutinee counterpart of `compile-sexpr-field`/
;; `compile-struct-field`: field `idx` of a boxed enum value, fetched via
;; `rt_data_field` (the enum peer of `rt_struct_field_get`) then decoded
;; through `compile-sexpr-field` per this field's own `kind`
;; (`Repr::field_kind`'s numbering — enum fields are tagged the
;; same way a struct's are now, see `translate_construct`'s doc comment).
(defun compile-box-field ((builder llvm-builder) (m llvm-module) (v llvm-value) (kind i32) (idx i32)) llvm-value
  (let ((args-ptr (alloca-args builder 2)))
    (store-arg builder args-ptr 0 v)
    (store-arg builder args-ptr 1 (const-word builder idx))
    (let ((raw (build-call builder (get-function m "rt_data_field") args-ptr 2)))
      (compile-sexpr-field builder m raw kind 0))))

;; The struct-scrutinee counterpart of `compile-sexpr-field`/
;; `compile-box-field`: field `idx` of a boxed struct `v` (already a properly
;; tagged `Sexpr`, unlike a sum-ADT box's untagged raw pointer -- there is no
;; `build-int-to-ptr` here) is fetched via `rt_struct_field_get` -- the exact
;; same call `compile-field-get` makes -- then decoded through
;; `compile-sexpr-field` per this field's own `kind`
;; (`Repr::field_kind`'s numbering, reused verbatim; a scalar/
;; passthrough kind's own `idx` parameter is unused, so `0` is passed, the
;; same convention `compile-field-get` follows).
(defun compile-struct-field ((builder llvm-builder) (m llvm-module) (v llvm-value) (kind i32) (idx i32)) llvm-value
  (let ((args-ptr (alloca-args builder 2)))
    (store-arg builder args-ptr 0 v)
    (store-arg builder args-ptr 1 (const-word builder idx))
    (let ((raw (build-call builder (get-function m "rt_struct_field_get") args-ptr 2)))
      (compile-sexpr-field builder m raw kind 0))))

;; The literal tagged `Sexpr::Str` "option", built directly from raw
;; Unicode scalar constants (`rt_str_new`'s own "raw char scalars"
;; contract — no `core_bridge::str_form` AST node needed) — for a
;; natively-compiled builtin that must synthesize a real `Option<T>` value
;; with no corresponding source-level `Option::some`/`none` call site to
;; derive a type-name form from (`try-bignum->int`'s `rt_data_new` calls
;; below being the one case today).
(defun compile-option-type-name ((builder llvm-builder) (m llvm-module)) llvm-value
  (let ((args-ptr (alloca-args builder 6)))
    (store-arg builder args-ptr 0 (const-word builder 111))
    (store-arg builder args-ptr 1 (const-word builder 112))
    (store-arg builder args-ptr 2 (const-word builder 116))
    (store-arg builder args-ptr 3 (const-word builder 105))
    (store-arg builder args-ptr 4 (const-word builder 111))
    (store-arg builder args-ptr 5 (const-word builder 110))
    (build-call builder (get-function m "rt_str_new") args-ptr 6)))

;; The `(some raw)` an always-succeeding conversion returns. `kind` is the
;; field's `Repr::field_kind` — the payload has to be tagged the way every
;; other struct/enum field is, `rt_data_new`'s contract being the same
;; tagged-field one `rt_struct_new` has.
(defun build-some-of ((builder llvm-builder) (m llvm-module) (raw llvm-value) (kind i32)) llvm-value
  (let ((some-args (alloca-args builder 3)))
    (store-arg builder some-args 0 (compile-option-type-name builder m))
    (store-arg builder some-args 1 (const-word builder 0))
    (store-arg builder some-args 2 (compile-tag-struct-field builder m raw kind))
    (let ((some-box (build-call builder (get-function m "rt_data_new") some-args 3)))
      (push-permanent-sexpr-root builder m some-box)
      some-box)))

;; The `(none)` of the same `Option`.
(defun build-none-of ((builder llvm-builder) (m llvm-module)) llvm-value
  (let ((none-args (alloca-args builder 2)))
    (store-arg builder none-args 0 (compile-option-type-name builder m))
    (store-arg builder none-args 1 (const-word builder 1))
    (let ((none-box (build-call builder (get-function m "rt_data_new") none-args 2)))
      (push-permanent-sexpr-root builder m none-box)
      none-box)))

;; The `Option` a `try-*` conversion returns: `(some raw)` when `fits` is
;; nonzero, `(none)` otherwise.
;;
;; Real control flow, the same `compile-if`-shaped "alloca a merge slot,
;; branch, store each arm's result, load after the merge block" shape
;; `compile-hashtable-op`'s `get`/`remove` uses. `raw` is computed by the
;; caller *before* the branch, which is safe only because every conversion
;; routed through here is pure (a normalization, an identity, or an `fptrunc`)
;; — `try-bignum->int` is not, and keeps its own arm where the raw call is
;; made after the test.
(defun build-try-option ((builder llvm-builder) (m llvm-module) (cur-fn llvm-function) (fits llvm-value) (raw llvm-value) (kind i32)) llvm-value
  (let* ((then-block (append-block cur-fn "try-some"))
         (else-block (append-block cur-fn "try-none"))
         (merge-block (append-block cur-fn "try-merge"))
         (slot (alloca-args builder 1)))
    (build-cond-br builder fits then-block else-block)
    (position-at-end builder then-block)
    (store-arg builder slot 0 (build-some-of builder m raw kind))
    (build-br builder merge-block)
    (position-at-end builder else-block)
    (store-arg builder slot 0 (build-none-of builder m))
    (build-br builder merge-block)
    (position-at-end builder merge-block)
    (load-raw builder slot 0)))

(defun compile-value ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    ;; The AST-tag dispatch. `case` (the core macro layer's
    ;; — `typelisp_front::core_macros`) rather than the
    ;; hand-nested `if` chain this used to be: the
    ;; expansion is literally the same `if` chain, so
    ;; nothing the checker, the interpreter or the
    ;; emitted IR sees changes — it trades 37 levels of
    ;; indentation for one flat clause list, and the key
    ;; is read once instead of per clause.
    ;;
    ;; This used to be an island-local `icond`/`icase`
    ;; copy, on the reading that this SOURCE may name only
    ;; builtins and special forms. That fence is about the
    ;; code the island *emits*: a prelude function call
    ;; would still be there afterwards, but a macro is
    ;; gone by then, and the island's SOURCE is checked
    ;; with the layer defining these already loaded. The
    ;; duplicates are deleted.
    ;;
    ;; The key is the tag *symbol*, not its name: `case`
    ;; literalizes a bare key, so each clause compares
    ;; interned symbols — which is a pointer comparison
    ;; (`typelisp-mem`'s `SymRef`), CL's `eq`. Reading the
    ;; name back out with `sexpr-sym-name` (as this did)
    ;; allocated a `string` on the GC heap once per
    ;; dispatch and then compared it 37 times by content.
    ;; The name is still read on the one path that needs
    ;; to *print* it, the `else` panic.
    ;;
    ;; A hash table keyed by tag was considered and
    ;; rejected: the arms are *code*, not values, so a
    ;; table would mean building 37 heap `ClosureBox`es
    ;; per `compile-function` call and dispatching
    ;; indirectly through them — strictly worse than the
    ;; linear `equal` chain, which does not register at
    ;; all against the ~1.7s it takes to AOT-compile this
    ;; entire island. (Name lookup — the thing a hash
    ;; table *is* right for — already is one: `env`/
    ;; `fn-env` are `Scope<V>`, `String`-keyed `HashMap`
    ;; frames.)
    (case (sexpr-car e)
      (int-any-width (compile-int-any-width m fn-name builder e))
      (char (compile-char m fn-name builder e))
      (bool (compile-bool m fn-name builder e))
      (float-any-width (compile-float-any-width m fn-name builder e))
      (str (compile-str m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (bignum (compile-bignum-literal m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (ratio (compile-ratio-literal m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (unit (compile-unit m fn-name builder))
      (var (compile-var m fn-name builder env fn-env captured e))
      (cellvar (compile-cellvar m fn-name builder env fn-env captured e))
      (llvm-op (compile-llvm-op m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (assoc (compile-assoc m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (apply (compile-apply m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (labels (compile-labels m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (call (compile-call m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (lambda (compile-lambda m fn-name builder env fn-env captured e))
      (apply-indirect (compile-apply-indirect m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (if (compile-if m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (let (compile-let m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (loop (compile-loop m fn-name builder env fn-env captured cur-fn protect exit-cleanup e))
      (break (compile-break m fn-name builder loop-exit loop-slot loop-root-base exit-cleanup))
      (return (compile-return m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (set (compile-set m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (cellset (compile-cellset m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (match (compile-match m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (construct (compile-construct m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (field-get (compile-field-get m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (field-set (compile-field-set m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (global (compile-global m fn-name builder e))
      (set-global (compile-set-global m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (global-init (compile-global-init m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (panic (compile-panic m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (catch (compile-catch m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (throw (compile-throw m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (unwind-protect (compile-unwind-protect m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (vector-op (compile-vector-op m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (hashtable-op (compile-hashtable-op m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (dyn-new (compile-dyn-new m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (dyn-call (compile-dyn-call m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (dyn-upcast (compile-dyn-upcast m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (dyn-value (compile-dyn-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup e))
      (else (panic (append "compile-value: unsupported tag " (sexpr-sym-name (sexpr-car e))))))
                           )

;; `(unit)` — `Expr::Unit`, represented (like every
;; other compiled value) as a plain `i64`; `0`, the
;; same encoding `compile-bool` already uses for
;; `false` (`registry::llvm_module_def`'s "every
;; compiled value is a plain i64" convention). Needed
;; now that `translate_return`'s value-less `(return)`
;; case desugars to an explicit `(unit)` value-form
;; rather than a second special-cased tag.
(defun compile-unit ((m llvm-module) (fn-name string) (builder llvm-builder))llvm-value
    (const-word builder 0))

;; The low 32 bits of `lo`, sign extension cleared -- the
;; shared tail of `compile-int-any-width` and `compile-float-any-width`, both of
;; which reassemble a 64-bit literal from two signed `i32`
;; halves. See `compile-int-any-width`'s doc comment for why a mask
;; constant can't be used instead.
(defun low-half-word ((builder llvm-builder) (lo i32)) llvm-value
    (build-lshr builder
                (build-shl builder (const-word builder lo) (const-word builder 32))
                (const-word builder 32)))

;; `(int HI LO)` -- two 32-bit halves, not one word, for
;; the same reason `compile-float-any-width` takes two: this island
;; reads a `Sexpr` int back through its 3-bit tag, so only
;; 61 bits survive the crossing and a wider literal arrived
;; sign-extended from bit 60. Reassembled here in LLVM,
;; where a full 64-bit word is ordinary -- doing it in the
;; island's own arithmetic would overflow the same way.
;;
;; Each half is a *bit pattern* read back as a signed `i32`
;; (`core_bridge::half`) -- a low half of `0xFFFF_FFFF` is
;; no `i32` number, and `sexpr-i32` hands back an `i32`
;; because that is the widest fixed-width integer the
;; language has. So `const-word` sign-extends both halves,
;; and the low one has to have that extension cleared
;; before the `or`. `shl 32` then `lshr 32` does it: the
;; obvious mask, `0xFFFF_FFFF`, is itself past `i32` and
;; could not be written here.
(defun compile-int-any-width ((m llvm-module) (fn-name string) (builder llvm-builder) (e Option<Sexpr>))llvm-value
    (let ((hi (sexpr-i32 (sexpr-car (sexpr-cdr e))))
          (lo (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
      (build-or builder
                (build-shl builder (const-word builder hi) (const-word builder 32))
                (low-half-word builder lo))))


;; `(char c)` -- a bare `char` literal. Compiled the same
;; way `compile-int-any-width` is (a plain, untagged `i64` scalar --
;; `char->int`'s own Unicode-scalar-value payload, widened
;; from its `i32` result via the zero-cost `i32`<->`i64`
;; `as` relabel `Checker::check_as` already allows), never
;; a tagged `Sexpr::Char` (that's `compile-construct-
;; sexpr`'s own variant-3 arm, which calls this exactly
;; like `compile-int-any-width`'s own variant-1 arm calls
;; `compile-int-any-width`). Added alongside `sexpr-char`
;; (`registry.rs`/`Interp::eval_builtin`) -- a bare
;; `char` literal previously had no dispatch tag at all
;; (an oversight discovered while implementing
;; `Expr::Quote`, whose `Char` leaf needs exactly this).
(defun compile-char ((m llvm-module) (fn-name string) (builder llvm-builder) (e Option<Sexpr>))llvm-value
    (const-word builder (char->int (sexpr-char (sexpr-car (sexpr-cdr e))))))

;; `(bool b)` (if/let/comparisons, labels/closures
;; Stage 5) — every compiled value is a plain `i64`
;; (see `registry::llvm_module_def`'s doc comment), so
;; a `bool` literal is just `0`/`1`, the same
;; representation `build-icmp-*` already produces and
;; `compile-if`'s `build-cond-br` already expects.
(defun compile-bool ((m llvm-module) (fn-name string) (builder llvm-builder) (e Option<Sexpr>))llvm-value
    (if (sexpr-bool (sexpr-car (sexpr-cdr e))) (const-word builder 1) (const-word builder 0)))

;; `(float-any-width hi lo)` (Sexpr/RtValue unification,
;; Stage 0) -- a bare float literal, of either width. `bits`
;; is the literal's raw `f64::to_bits` pattern embedded as
;; two plain integer halves by `core_bridge`'s
;; `float-any-width` node. Like `compile-int-any-width`, this returns
;; the *plain, untagged* bit pattern -- **not** a boxed
;; a float `Sexpr` node (that would be `rt_f64_new`,
;; wrongly called twice: once here, once more by
;; `compile-construct-sexpr`'s own variant-2 arm on
;; the tagged result this function already returned --
;; boxing belongs to construct-sexpr alone, the exact
;; same division of labor `compile-int-any-width`/variant-1
;; already has: `compile-int-any-width` returns a bare `i64`,
;; `compile-construct-sexpr`'s `build-shl`/tag-OR is
;; what actually makes it a `Sexpr::i32`).
;; `bits` arrives as two 32-bit halves `(float-any-width hi lo)`
;; (`core_bridge`'s `float-any-width` node, interp-closure removal
;; Stage 8a): a single tagged `Sexpr` `i32` would lose
;; the top 3 bits of a full-width `f64` pattern when
;; read back here (`sexpr-i32` = `>> 3`), decoding e.g.
;; `2.0` to `0.0`. Reassemble with `(hi << 32) | lo` —
;; LLVM constant-folds it back to the exact 64-bit
;; pattern. `low-half-word` clears the low half's sign
;; extension; see `compile-int-any-width` for why that is needed.
(defun compile-float-any-width ((m llvm-module) (fn-name string) (builder llvm-builder) (e Option<Sexpr>))llvm-value
    (let ((hi (sexpr-i32 (sexpr-car (sexpr-cdr e))))
          (lo (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
      (build-or builder
                (build-shl builder (const-word builder hi) (const-word builder 32))
                (low-half-word builder lo))))

;; `(str (int c0) (int c1) ...)` (Stage 7 of the
;; Sexpr-representation plan, `docs/implementation-log.md`)
;; — a string literal's content, one `(int c)` node per
;; character (`core_bridge::str_form`'s doc comment
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
(defun compile-str ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((chars (sexpr-cdr e))
           (n (sexpr-list-length chars))
           (args-ptr (alloca-args builder n)))
      (store-str-chars m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr chars 0)
      (build-call builder (get-function m "rt_str_new") args-ptr n)))

;; Fills a `compile-str`-allocated array, one compiled
;; `(int c)` character per slot — the `str`-literal
;; analogue of `compile-construct-box-fields`.
(defun store-str-chars ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (args-ptr llvm-value) (forms Option<Sexpr>) (idx i32))()
    (if (sexpr-consp forms)
        (let ((form (sexpr-car forms)) (rest (sexpr-cdr forms)))
          (store-arg builder args-ptr idx (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup form))
          (store-str-chars m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr rest (+ idx 1)))
        ()))

;; `(bignum (int sign) (int d0) (int d1) ...)` —
;; `core_bridge::bignum_form`'s doc comment.
;; Same shape as `compile-str`/`store-str-chars` (one
;; `(int _)` node per raw payload scalar, reused
;; verbatim to fill the args array), calling
;; `rt_bignum_new` instead of `rt_str_new`. Like `str`,
;; the result is already a fully tagged `Sexpr::Bignum`
;; — no further bit manipulation needed at the
;; `compile-construct-sexpr`/`compile-sexpr-field`
;; boundary (both treat variant `8` as passthrough).
(defun compile-bignum-literal ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((parts (sexpr-cdr e))
           (n (sexpr-list-length parts))
           (args-ptr (alloca-args builder n)))
      (store-str-chars m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr parts 0)
      (build-call builder (get-function m "rt_bignum_new") args-ptr n)))

;; `(ratio numer-form denom-form)` —
;; `core_bridge`'s `ratio` arm. Both
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
(defun compile-ratio-literal ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let ((numer-form (sexpr-car (sexpr-cdr e))) (denom-form (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
      (let* ((args-ptr (alloca-args builder 2))
             (numer-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup numer-form)))
        (store-arg builder args-ptr 0 numer-v)
        (push-sexpr-root builder m numer-v)
        (let ((denom-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup denom-form)))
          (store-arg builder args-ptr 1 denom-v)
          (push-sexpr-root builder m denom-v)
          (let ((result (build-call builder (get-function m "rt_ratio_from_bignums") args-ptr 2)))
            (pop-sexpr-root builder m)
            (pop-sexpr-root builder m)
            result)))))

;; `(var name is-fn)` — a plain variable reference,
;; ordinary or a `labels` sibling/self referenced *as
;; a value* rather than called (e.g. a `labels`
;; block's own trailing body bare-returning one of
;; its siblings — `core_bridge::to_island`'s
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
(defun compile-var ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (e Option<Sexpr>))llvm-value
    (resolve-value m fn-name builder env fn-env captured (sexpr-str (sexpr-car (sexpr-cdr e)))))

;; `(cellvar name kind)` — a reference to a cell-boxed
;; name (`core_bridge`'s `Ctx::cell_names`, closure-
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
;; (`core_bridge` only ever emits this tag for a name it
;; already knows is a cell-boxed local/param/capture).
(defun compile-cellvar ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (e Option<Sexpr>))llvm-value
    (let* ((name (sexpr-str (sexpr-car (sexpr-cdr e))))
           (kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
      (match (get env name)
        ((Some slot)
         (let* ((cell-ref (load-raw builder slot 0))
                (args-ptr (alloca-args builder 1)))
           (store-arg builder args-ptr 0 cell-ref)
           (let ((raw (build-call builder (get-function m "rt_cell_get") args-ptr 1)))
             (compile-sexpr-field builder m raw kind 0))))
        (None (panic (append "compile-cellvar: unbound variable " name))))))

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
(defun resolve-value ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (name string))llvm-value
    (match (get env name)
      ((Some slot) (load-raw builder slot 0))
      (None (match (get fn-env name)
              ((Some target)
               (let* ((env-len (sexpr-list-length captured))
                      (env-ptr (alloca-args builder env-len)))
                 (compile-escaping-env-args m fn-name builder env fn-env captured env-ptr captured 0)
                 (build-make-closure builder m target env-ptr env-len (compute-sexpr-mask-part captured 0 0) (compute-sexpr-mask-part captured 0 32))))
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
(defun compile-env-args ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (env-ptr llvm-value) (names Option<Sexpr>) (idx i32))()
    (if (sexpr-consp names)
        (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
         (let* ((nm (sexpr-sym-name (sexpr-car name-pair)))
                (v (resolve-value m fn-name builder env fn-env captured nm)))
           (store-arg builder env-ptr idx v)
           (compile-env-args m fn-name builder env fn-env captured env-ptr rest (+ idx 1))))
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
(defun compile-escaping-env-args ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (env-ptr llvm-value) (names Option<Sexpr>) (idx i32))()
    (if (sexpr-consp names)
        (let ((name-pair (sexpr-car names)) (rest (sexpr-cdr names)))
         (let* ((nm (sexpr-sym-name (sexpr-car name-pair)))
                (v (resolve-value m fn-name builder env fn-env captured nm)))
           (store-arg builder env-ptr idx v)
           (compile-escaping-env-args m fn-name builder env fn-env captured env-ptr rest (+ idx 1))))
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
;; (`core_bridge::translate_assoc` builds them with
;; `arg_pairs`, which pairs each argument with its
;; repr), so a receiver/argument that's itself
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
(defun compile-assoc ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((type-name (sexpr-str (sexpr-car (sexpr-cdr e))))
           (method (sexpr-str (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
           (rest (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
      (cond
        ((if (equal type-name "sexpr") (sexpr-native-method? method) false)
         (let* ((a (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car rest))))
                (b (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
           (if (equal method "eq")
               (build-icmp-eq builder a b)
               (let ((args-ptr (alloca-args builder 2)))
                 (store-arg builder args-ptr 0 a)
                 (store-arg builder args-ptr 1 b)
                 (build-call builder (get-function m "rt_sexpr_eql") args-ptr 2)))))
        ((if (equal type-name "string") (string-native-method? method) false)
         (let ((a (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car rest)))))
           (if (equal method "length")
               (let ((args-ptr (alloca-args builder 1)))
                 (store-arg builder args-ptr 0 a)
                 (build-call builder (get-function m "rt_str_length") args-ptr 1))
           (if (string-unary-shim-method? method)
               (let ((args-ptr (alloca-args builder 1)))
                 (store-arg builder args-ptr 0 a)
                 (build-call builder (get-function m (string-unary-shim-name method)) args-ptr 1))
               (let ((b (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
                 (case method
                   ;; `ref` can raise (an out-of-range index), so it goes
                   ;; through `raising-binop-call` rather than a bare
                   ;; `build-call` — see that function.
                   ("ref"
                    (raising-binop-call builder m cur-fn protect "rt_str_ref" a b))
                   (("eq" "eql")
                    ;; `eq`/`eql` on a string are *identity*
                    ;; (`string_identity_eq`), and a compiled string is
                    ;; the tagged `Value::Str` word, so identity is an
                    ;; `icmp eq` on that word. These two shared
                    ;; `rt_str_eq` (content) with `equal` until
                    ;; 2026-08-18, which made compiled `(eq a b)`
                    ;; answer true where the interpreter said false.
                    (build-icmp-eq builder a b))
                   ("equal"
                    ;; `equal`: case-sensitive *content* comparison
                    ;; (`rt_str_eq`) — the prelude's `impl Eq string`
                    ;; body.
                    (let ((args-ptr (alloca-args builder 2)))
                      (store-arg builder args-ptr 0 a)
                      (store-arg builder args-ptr 1 b)
                      (build-call builder (get-function m "rt_str_eq") args-ptr 2)))
                   ;; `equalp`: ASCII case-insensitive content equality
                   ;; (`rt_str_equalp`) — no in-place lowering, unlike
                   ;; `eq`/`equal`'s plain content compare above.
                   ("equalp"
                    (let ((args-ptr (alloca-args builder 2)))
                      (store-arg builder args-ptr 0 a)
                      (store-arg builder args-ptr 1 b)
                      (build-call builder (get-function m "rt_str_equalp") args-ptr 2)))
                   ;; `<`/`>`/`<=`/`>=` all derive from `rt_str_lt`
                   ;; (`str-lt-call`); `not` is `(icmp-eq v 0)`.
                   (("lt" "<")
                    (str-lt-call builder m a b))
                   (">"
                    (str-lt-call builder m b a))
                   ("<="
                    (build-icmp-eq builder (str-lt-call builder m b a) (const-word builder 0)))
                   (">="
                    (build-icmp-eq builder (str-lt-call builder m a b) (const-word builder 0)))
                   ("append"
                    (let ((args-ptr (alloca-args builder 2)))
                      (store-arg builder args-ptr 0 a)
                      (store-arg builder args-ptr 1 b)
                      (build-call builder (get-function m "rt_str_append") args-ptr 2)))
                   ;; `substring`: the one three-operand string method —
                   ;; `b` is the start index and the third operand the
                   ;; end, both raw `i32`s (`rt_str_ref`'s own index
                   ;; convention). Compiled here rather than beside `b`
                   ;; above so no other method pays for reading an
                   ;; argument form it doesn't have. Like `ref` it can
                   ;; raise (an invalid range), so the call is protected;
                   ;; `raising-binop-call` is two-operand only, hence the
                   ;; `emit-direct-call` spelled out here.
                   ("substring"
                    (let* ((c (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car (sexpr-cdr (sexpr-cdr rest))))))
                           (args-ptr (alloca-args builder 3)))
                      (store-arg builder args-ptr 0 a)
                      (store-arg builder args-ptr 1 b)
                      (store-arg builder args-ptr 2 c)
                      (emit-direct-call builder m cur-fn (get-function m "rt_str_substring") args-ptr 3 protect)))
                   (else (panic (append "compile-assoc: unsupported str method " method)))))))))
        ((if (int-receiver-type? type-name) (int-native-method? method) false)
         (let ((a (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car rest))))
               (wsig (int-wsig type-name)))
           ;; `int->bignum`/`int->ratio`: unary, checked before `b2`
           ;; is read — same reason `float-native-method?`'s own
           ;; unary conversions are checked first (there is no
           ;; second argument form to compile for these).
           (case method
             ("int->bignum"
              (let ((args-ptr (alloca-args builder 1)))
                (store-arg builder args-ptr 0 a)
                (build-call builder (get-function m "rt_int_to_bignum") args-ptr 1)))
             ("int->ratio"
              (let ((args-ptr (alloca-args builder 1)))
                (store-arg builder args-ptr 0 a)
                (build-call builder (get-function m "rt_int_to_ratio") args-ptr 1)))
             ("int->float"
              (build-sitofp builder a))
             ("int->char"
              (let ((args-ptr (alloca-args builder 1)))
                (store-arg builder args-ptr 0 a)
                (emit-direct-call builder m cur-fn (get-function m "rt_int_to_char") args-ptr 1 protect)))
             ;; A width cast is the normalization on its own: cut the value to
             ;; the *target*'s width and re-extend. `int-wsig` reads the target
             ;; out of the method name, which is the only place it is written.
             (("int->i8" "int->i16" "int->i32" "int->u8" "int->u16" "int->u32")
              (build-normalize-int builder a (int-wsig (substring method 5 (length method)))))
             ;; The `Option`-returning halves of those two. `rt_int_fits*`
             ;; answers the question and nothing else: a value that fits its
             ;; target is *unchanged* by the cast, so the `some` payload is
             ;; the operand already in hand rather than a converted one.
             ("try-int->char"
              (let ((fits-args (alloca-args builder 1)))
                (store-arg builder fits-args 0 a)
                (build-try-option builder m cur-fn
                  (build-call builder (get-function m "rt_int_fits_char") fits-args 1) a 3)))
             (("try-int->i8" "try-int->i16" "try-int->i32" "try-int->u8" "try-int->u16" "try-int->u32")
              (let ((fits-args (alloca-args builder 2)))
                (store-arg builder fits-args 0 a)
                (store-arg builder fits-args 1 (const-word builder (int-wsig (substring method 9 (length method)))))
                (build-try-option builder m cur-fn
                  (build-call builder (get-function m "rt_int_fits") fits-args 2) a 1)))
             ("logcount"
              (int-unary-shim-call builder m "rt_int_logcount" a wsig))
             ("integer-length"
              (int-unary-shim-call builder m "rt_int_integer_length" a wsig))
             ;; `lognot` is `xor -1`, but the complement of a normalized
             ;; value is not itself normalized — `(lognot (the u8 0))` is
             ;; `255`, and the raw `xor` gives `-1`.
             ("lognot"
              (build-normalize-int builder (build-xor builder a (const-word builder -1)) wsig))
             (else (let ((b2 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
                      (case method
                        ;; The low `w` bits of a 64-bit sum are the sum's
                        ;; own low `w` bits, so the wide instruction is a
                        ;; correct way to compute the narrow one — as long as
                        ;; the bits above `w` are put back the way the type
                        ;; says (`build-normalize-int`).
                        ("+"
                         (build-normalize-int builder (build-add builder a b2) wsig))
                        ("-"
                         (build-normalize-int builder (build-sub builder a b2) wsig))
                        ("*"
                         (build-normalize-int builder (build-mul builder a b2) wsig))
                        ;; `/` and `mod`: unlike `+`/`-`/`*`, integer
                        ;; division can't be a bare LLVM instruction —
                        ;; `sdiv`/`srem` by zero is UB — so both route
                        ;; through `rt_int_div`/`rt_int_mod`, which
                        ;; check the divisor (and `MIN/-1`) and raise
                        ;; the interpreter's own `divide by zero`/`mod
                        ;; by zero`. Raising means unwinding, so the
                        ;; call is made through `raising-binop-call`.
                        ("/"
                         (raising-int-binop-call builder m cur-fn protect "rt_int_div" a b2 wsig))
                        ("mod"
                         (raising-int-binop-call builder m cur-fn protect "rt_int_mod" a b2 wsig))
                        ("<"
                         (build-icmp-lt builder a b2))
                        ("<="
                         (build-icmp-le builder a b2))
                        (">"
                         (build-icmp-gt builder a b2))
                        (">="
                         (build-icmp-ge builder a b2))
                        ;; The five names CL gives one `icmp eq` on two
                        ;; integers — `int-native-method?` lists the same
                        ;; set, which is what makes them reachable here.
                        (("=" "eq" "eql" "equal" "equalp")
                         (build-icmp-eq builder a b2))
                        ("/="
                         (build-icmp-ne builder a b2))
                        ("logand"
                         (build-and builder a b2))
                        ("logior"
                         (build-or builder a b2))
                        ("logxor"
                         (build-xor builder a b2))
                        ("logtest"
                         (build-icmp-ne builder (build-and builder a b2) (const-word builder 0)))
                        ("max"
                         (build-select builder (build-icmp-gt builder a b2) a b2))
                        ("min"
                         (build-select builder (build-icmp-lt builder a b2) a b2))
                        ("ash"
                         (int-binop-shim-call builder m "rt_int_ash" a b2 wsig))
                        ("logbitp"
                         (int-binop-shim-call builder m "rt_int_logbitp" a b2 wsig))
                        (else (panic (append "compile-assoc: unsupported method " method)))))))))
        ((if (equal type-name "char") (char-native-method? method) false)
         (let ((a (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car rest)))))
           ;; `char->int` is unary (receiver only) and the
           ;; identity at the compiled level — return the
           ;; receiver's raw code point unchanged, before
           ;; the binary branch below tries to read a
           ;; (non-existent) second operand. `char->string`
           ;; is unary for the same reason: a one-character
           ;; `rt_str_new`, whose arguments are raw code
           ;; points, which is exactly what `a` already is.
           (if (equal method "char->int")
               a
           (if (equal method "char->string")
               (let ((args-ptr (alloca-args builder 1)))
                 (store-arg builder args-ptr 0 a)
                 (build-call builder (get-function m "rt_str_new") args-ptr 1))
           (if (char-unary-shim-method? method)
               (unary-shim-call builder m (char-unary-shim-name method) a)
           (let ((b2 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
             (case method
               ("equalp"
                (let ((args-ptr (alloca-args builder 2)))
                  (store-arg builder args-ptr 0 a)
                  (store-arg builder args-ptr 1 b2)
                  (build-call builder (get-function m "rt_char_equalp") args-ptr 2)))
               (("lt" "<")
                (build-icmp-lt builder a b2))
               ("<="
                (build-icmp-le builder a b2))
               (">"
                (build-icmp-gt builder a b2))
               (">="
                (build-icmp-ge builder a b2))
               (else (build-icmp-eq builder a b2)))))))))
        ;; `bool`/`symbol`: one `icmp eq` each, over a raw `0`/`1` and
        ;; over an interned handle respectively. Both receivers reach
        ;; `compile-assoc` with the type name the checker spells
        ;; (`prim_type_path`), so neither can be folded into the `sexpr`
        ;; arm above even though `symbol`'s lowering is identical to it.
        ((if (equal type-name "bool") (bool-native-method? method) false)
         (let* ((a (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car rest))))
                (b2 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
           (build-icmp-eq builder a b2)))
        ((if (equal type-name "symbol") (symbol-native-method? method) false)
         (let* ((a (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car rest))))
                (b2 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
           (build-icmp-eq builder a b2)))
        ((if (float-receiver-type? type-name) (float-native-method? method) false)
         (let ((a (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car rest)))))
           (round-f32-if builder type-name method
           (case method
             ("float->f32"
              (build-fround32 builder a))
             ;; Widening is exact and needs no instruction: the value in the
             ;; register is already the nearest binary32 one.
             ("float->f64"
              a)
             ;; `try-float->f32` narrows only when the value survives the
             ;; round trip; `try-float->f64` widens, which is always exact, so
             ;; it has no question to ask and emits the `some` directly rather
             ;; than branching on a constant.
             ("try-float->f32"
              (let ((fits-args (alloca-args builder 1)))
                (store-arg builder fits-args 0 a)
                (build-try-option builder m cur-fn
                  (build-call builder (get-function m "rt_f64_fits_f32") fits-args 1)
                  (build-fround32 builder a) 11)))
             ("try-float->f64"
              (build-some-of builder m a 2))
             ("sqrt"
              (build-fsqrt builder m a))
             ("floor"
              (build-ffloor builder m a))
             ("ceiling"
              (build-fceil builder m a))
             ("round"
              (build-fround builder m a))
             ("truncate"
              (build-ftrunc builder m a))
             ("float->int"
              (build-fptosi builder m a))
             ("float->bignum"
              (let ((args-ptr (alloca-args builder 1)))
                (store-arg builder args-ptr 0 a)
                (build-call builder (get-function m "rt_float_to_bignum") args-ptr 1)))
             ("float->ratio"
              (let ((args-ptr (alloca-args builder 1)))
                (store-arg builder args-ptr 0 a)
                (build-call builder (get-function m "rt_float_to_ratio") args-ptr 1)))
             ("sin"
              (build-fsin builder m a))
             ("cos"
              (build-fcos builder m a))
             ("exp"
              (build-fexp builder m a))
             ("log"
              (build-flog builder m a))
             ("tan"
              (unary-shim-call builder m "rt_f64_tan" a))
             ("asin"
              (unary-shim-call builder m "rt_f64_asin" a))
             ("acos"
              (unary-shim-call builder m "rt_f64_acos" a))
             ("atan"
              (unary-shim-call builder m "rt_f64_atan" a))
             ("sinh"
              (unary-shim-call builder m "rt_f64_sinh" a))
             ("cosh"
              (unary-shim-call builder m "rt_f64_cosh" a))
             ("tanh"
              (unary-shim-call builder m "rt_f64_tanh" a))
             ("asinh"
              (unary-shim-call builder m "rt_f64_asinh" a))
             ("acosh"
              (unary-shim-call builder m "rt_f64_acosh" a))
             ("atanh"
              (unary-shim-call builder m "rt_f64_atanh" a))
             (else (let ((b2 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
                      (case method
                        ("+"
                         (build-fadd builder a b2))
                        ("-"
                         (build-fsub builder a b2))
                        ("*"
                         (build-fmul builder a b2))
                        ("/"
                         (build-fdiv builder a b2))
                        ("mod"
                         (build-frem builder a b2))
                        ("expt"
                         (build-fpow builder m a b2))
                        ("<"
                         (build-fcmp-lt builder a b2))
                        ("<="
                         (build-fcmp-le builder a b2))
                        (">"
                         (build-fcmp-gt builder a b2))
                        (">="
                         (build-fcmp-ge builder a b2))
                        ("/="
                         (build-fcmp-ne builder a b2))
                        ("max"
                         (build-fmaxnum builder m a b2))
                        ("min"
                         (build-fminnum builder m a b2))
                        (else (build-fcmp-eq builder a b2)))))))))
        ((if (equal type-name "bignum") (bignum-native-method? method) false)
         (let ((a (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car rest)))))
           (case method
             ("bignum->int"
              (bignum-unary-call builder m "rt_bignum_to_int" a))
             ("bignum->float"
              (bignum-unary-call builder m "rt_bignum_to_float" a))
             ("bignum->ratio"
              (bignum-unary-call builder m "rt_bignum_to_ratio" a))
             ;; `lognot` is unary, so — like the conversions above — it is
             ;; checked before `b2` is read.
             ("lognot"
              (bignum-unary-call builder m "rt_bignum_lognot" a))
             ("logcount"
              (bignum-unary-call builder m "rt_bignum_logcount" a))
             ("integer-length"
              (bignum-unary-call builder m "rt_bignum_integer_length" a))
             ("try-bignum->int"
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
                (let* ((fits (build-call builder (get-function m "rt_bignum_fits_i32") fits-args 1))
                       (then-block (append-block cur-fn "bignum-fits"))
                       (else-block (append-block cur-fn "bignum-overflow"))
                       (merge-block (append-block cur-fn "bignum-try-merge"))
                       (slot (alloca-args builder 1)))
                  (build-cond-br builder fits then-block else-block)
                  (position-at-end builder then-block)
                  (let* ((raw (build-call builder (get-function m "rt_bignum_to_int_raw") fits-args 1))
                         (some-args (alloca-args builder 3)))
                    (store-arg builder some-args 0 (compile-option-type-name builder m))
                    (store-arg builder some-args 1 (const-word builder 0))
                    (store-arg builder some-args 2 (compile-tag-struct-field builder m raw 1))
                    (let ((some-box (build-call builder (get-function m "rt_data_new") some-args 3)))
                      (push-permanent-sexpr-root builder m some-box)
                      (let ((ignored (store-arg builder slot 0 some-box)))
                        (build-br builder merge-block))))
                  (position-at-end builder else-block)
                  (let ((none-args (alloca-args builder 2)))
                    (store-arg builder none-args 0 (compile-option-type-name builder m))
                    (store-arg builder none-args 1 (const-word builder 1))
                    (let ((none-box (build-call builder (get-function m "rt_data_new") none-args 2)))
                      (push-permanent-sexpr-root builder m none-box)
                      (let ((ignored (store-arg builder slot 0 none-box)))
                        (build-br builder merge-block))))
                  (position-at-end builder merge-block)
                  (load-raw builder slot 0))))
             (else (let ((b2 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
                      (case method
                        ("+"
                         (bignum-binop-call builder m "rt_bignum_add" a b2))
                        ("-"
                         (bignum-binop-call builder m "rt_bignum_sub" a b2))
                        ("*"
                         (bignum-binop-call builder m "rt_bignum_mul" a b2))
                        ;; The two that can raise on a zero divisor —
                        ;; `raising-binop-call`, not `bignum-binop-call`.
                        ("/"
                         (raising-binop-call builder m cur-fn protect "rt_bignum_div" a b2))
                        ("mod"
                         (raising-binop-call builder m cur-fn protect "rt_bignum_mod" a b2))
                        ("logand"
                         (bignum-binop-call builder m "rt_bignum_logand" a b2))
                        ("logior"
                         (bignum-binop-call builder m "rt_bignum_logior" a b2))
                        ("logxor"
                         (bignum-binop-call builder m "rt_bignum_logxor" a b2))
                        ("ash"
                         (bignum-binop-call builder m "rt_bignum_ash" a b2))
                        ("logbitp"
                         (bignum-binop-call builder m "rt_bignum_logbitp" a b2))
                        ("logtest"
                         (bignum-binop-call builder m "rt_bignum_logtest" a b2))
                        ("<"
                         (build-icmp-lt builder (bignum-cmp-call builder m a b2) (const-word builder 0)))
                        ("<="
                         (build-icmp-le builder (bignum-cmp-call builder m a b2) (const-word builder 0)))
                        (">"
                         (build-icmp-gt builder (bignum-cmp-call builder m a b2) (const-word builder 0)))
                        (">="
                         (build-icmp-ge builder (bignum-cmp-call builder m a b2) (const-word builder 0)))
                        ("/="
                         (build-icmp-ne builder (bignum-cmp-call builder m a b2) (const-word builder 0)))
                        ("max"
                         (build-select builder (build-icmp-ge builder (bignum-cmp-call builder m a b2) (const-word builder 0)) a b2))
                        ("min"
                         (build-select builder (build-icmp-le builder (bignum-cmp-call builder m a b2) (const-word builder 0)) a b2))
                        (else (build-icmp-eq builder (bignum-cmp-call builder m a b2) (const-word builder 0)))))))))
        ((if (equal type-name "ratio") (ratio-native-method? method) false)
         (let ((a (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car rest)))))
           (case method
             ("ratio->bignum"
              (ratio-unary-call builder m "rt_ratio_to_bignum" a))
             ("ratio->float"
              (ratio-unary-call builder m "rt_ratio_to_float" a))
             ("numerator"
              (ratio-unary-call builder m "rt_ratio_numerator" a))
             ("denominator"
              (ratio-unary-call builder m "rt_ratio_denominator" a))
             (else (let ((b2 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-cdr (sexpr-car (sexpr-cdr rest))))))
                      (case method
                        ("+"
                         (ratio-binop-call builder m "rt_ratio_add" a b2))
                        ("-"
                         (ratio-binop-call builder m "rt_ratio_sub" a b2))
                        ("*"
                         (ratio-binop-call builder m "rt_ratio_mul" a b2))
                        ;; Zero divisor: raises, so protected
                        ;; (`raising-binop-call`), like `bignum`'s `/`.
                        ("/"
                         (raising-binop-call builder m cur-fn protect "rt_ratio_div" a b2))
                        ("<"
                         (build-icmp-lt builder (ratio-cmp-call builder m a b2) (const-word builder 0)))
                        ("<="
                         (build-icmp-le builder (ratio-cmp-call builder m a b2) (const-word builder 0)))
                        (">"
                         (build-icmp-gt builder (ratio-cmp-call builder m a b2) (const-word builder 0)))
                        (">="
                         (build-icmp-ge builder (ratio-cmp-call builder m a b2) (const-word builder 0)))
                        ("/="
                         (build-icmp-ne builder (ratio-cmp-call builder m a b2) (const-word builder 0)))
                        ("max"
                         (build-select builder (build-icmp-ge builder (ratio-cmp-call builder m a b2) (const-word builder 0)) a b2))
                        ("min"
                         (build-select builder (build-icmp-le builder (ratio-cmp-call builder m a b2) (const-word builder 0)) a b2))
                        (else (build-icmp-eq builder (ratio-cmp-call builder m a b2) (const-word builder 0)))))))))
        (else (compile-assoc-user m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup type-name method rest)))))

;; The user-defined-method leg of `compile-assoc`'s
;; dispatch (see its doc comment): call the callee
;; under the mangled name `type-name::method`,
;; arguments through `compile-call-args` exactly like
;; an ordinary call's. A `labels` sibling (not a
;; toplevel `defun`) because it closes over `m` and
;; mutually recurses with `compile-call-args`.
(defun compile-assoc-user ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (type-name string) (method string) (rest Option<Sexpr>))llvm-value
    (let* ((mangled (append "tl_" (append type-name (append "::" method))))
           (argc (sexpr-list-length rest))
           (args-ptr (alloca-args builder argc))
           (sexpr-roots (compile-call-args m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr rest 0))
           (result (emit-direct-call builder m cur-fn (get-function m mangled) args-ptr argc protect)))
      (pop-sexpr-roots builder m sexpr-roots)
      result))

;; `(llvm-op opid (kind . arg)...)` — an `llvm-*`/
;; native-`Scope<V>` builtin method call
;; (`core_bridge::translate_assoc`'s lowering, interp-
;; closure removal Stage 1): one call to the generic
;; Rust-side dispatch shim `rt_llvm_call`, passing the
;; translate-time-resolved op id (`symbols::
;; llvm_op_id`'s stable hash) in slot 0 and the
;; compiled arguments after it. Argument handling is
;; exactly `compile-assoc-user`'s (`compile-call-args`
;; roots the tagged-`Sexpr`-kind ones across later
;; arguments' own allocations); the op id itself is a
;; raw constant with nothing to root. The result's
;; encoding (handle / unit / bool / tagged str /
;; boxed `Option`) is dictated by the node's checked
;; type, the same as every other compiled value.
(defun compile-llvm-op ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((opid (sexpr-i32 (sexpr-car (sexpr-cdr e))))
           (arg-forms (sexpr-cdr (sexpr-cdr e)))
           (argc (+ (sexpr-list-length arg-forms) 1))
           (args-ptr (alloca-args builder argc)))
      (store-arg builder args-ptr 0 (const-word builder opid))
      (let* ((sexpr-roots (compile-call-args m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr arg-forms 1))
             (result (build-call builder (get-function m "rt_llvm_call") args-ptr argc)))
        (pop-sexpr-roots builder m sexpr-roots)
        result)))

;; `(dyn-new vtable-id (kind . value-form))` — box a
;; concrete value as a trait object (`Expr::DynBox`,
;; TODO T4). The vtable id is a translate-time
;; constant (`core_bridge::translate_dyn_new`), so this
;; is just `rt_dyn_new(id, value)`. Argument handling
;; is `compile-llvm-op`'s: slot 0 holds the raw
;; constant, `compile-call-args` fills the rest and
;; roots the tagged-`Sexpr` ones across the
;; allocation `rt_dyn_new` itself performs.
(defun compile-dyn-new ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((vtable-id (sexpr-i32 (sexpr-car (sexpr-cdr e))))
           (arg-forms (sexpr-cdr (sexpr-cdr e)))
           (args-ptr (alloca-args builder 2)))
      (store-arg builder args-ptr 0 (const-word builder vtable-id))
      (let* ((sexpr-roots (compile-call-args m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr arg-forms 1))
             (result (build-call builder (get-function m "rt_dyn_new") args-ptr 2)))
        (pop-sexpr-roots builder m sexpr-roots)
        result)))

;; `(dyn-upcast trait-id (kind . value-form))` —
;; convert a trait object to a supertrait whose vtable
;; layout is not a prefix of the source's
;; (`Expr::DynUpcast`). Identical in shape to
;; `compile-dyn-new`, and for the same reason: slot 0
;; is a translate-time constant naming the target and
;; `rt_dyn_upcast` does the rest (it looks the target
;; *table* up from the box's own vtable id, which is
;; the only thing that still knows the concrete type).
;; The value argument is a `kind = 2` `Sexpr`, so
;; `compile-call-args` roots it across the allocation
;; `rt_dyn_upcast` performs — which is also what keeps
;; the concrete value inside it alive.
(defun compile-dyn-upcast ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((trait-id (sexpr-i32 (sexpr-car (sexpr-cdr e))))
           (arg-forms (sexpr-cdr (sexpr-cdr e)))
           (args-ptr (alloca-args builder 2)))
      (store-arg builder args-ptr 0 (const-word builder trait-id))
      (let* ((sexpr-roots (compile-call-args m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr arg-forms 1))
             (result (build-call builder (get-function m "rt_dyn_upcast") args-ptr 2)))
        (pop-sexpr-roots builder m sexpr-roots)
        result)))

;; `(dyn-value (kind . form))` — unwrap a trait object
;; to the concrete value inside (`Expr::DynValue`).
(defun compile-dyn-value ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((arg-forms (sexpr-cdr e))
           (args-ptr (alloca-args builder 1))
           (sexpr-roots (compile-call-args m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr arg-forms 0))
           (result (build-call builder (get-function m "rt_dyn_value") args-ptr 1)))
      (pop-sexpr-roots builder m sexpr-roots)
      result))

;; `(dyn-call slot (kind . recv-form) (kind . arg-form)...)`
;; — the dynamic dispatch itself (`Expr::DynCall`).
;; The slot is a translate-time constant, so this is
;; three steps and no search: read the receiver's
;; vtable id (`rt_dyn_vtable`), unwrap the concrete
;; receiver (`rt_dyn_value`), and hand both the slot
;; and the argument array to `rt_dyn_call`.
;;
;; `rt_dyn_call` rather than the `rt_vtable_slot` +
;; `build-dyn-call` pair this used to emit (both since
;; deleted): the implementation behind a slot
;; is not necessarily compiled (a compiled prelude
;; stream method dispatching on a `:dyn CharInput`
;; whose concrete type is the user's own struct), and
;; only the runtime can see which case it is — see
;; that shim's doc comment. The argument array is
;; passed as a pointer-sized integer, the same way
;; `build-closure-apply` hands `rt_apply_any` its own.
;;
;; The callee is an ordinary compiled method and
;; expects the *concrete* receiver, so slot 0 of the
;; argument array holds `rt_dyn_value` of the box
;; rather than the box itself — the same value a
;; static `compile-assoc-user` call would have passed.
;; The box stays rooted across the whole sequence
;; (`compile-call-args` pushed it as a `kind = 2`
;; argument), which is what keeps the unwrapped
;; concrete value alive too.
(defun compile-dyn-call ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((slot (sexpr-i32 (sexpr-car (sexpr-cdr e))))
           (arg-forms (sexpr-cdr (sexpr-cdr e)))
           (argc (sexpr-list-length arg-forms))
           (args-ptr (alloca-args builder argc))
           (sexpr-roots (compile-call-args m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr arg-forms 0))
           (recv (load-raw builder args-ptr 0))
           (vt-ptr (alloca-args builder 1)))
      (store-arg builder vt-ptr 0 recv)
      (let* ((vtable-id (build-call builder (get-function m "rt_dyn_vtable") vt-ptr 1))
             (inner (build-call builder (get-function m "rt_dyn_value") vt-ptr 1)))
        (store-arg builder args-ptr 0 inner)
        (let ((call-ptr (alloca-args builder 4)))
          (store-arg builder call-ptr 0 vtable-id)
          (store-arg builder call-ptr 1 (const-word builder slot))
          (store-arg builder call-ptr 2 (build-ptr-to-int builder args-ptr))
          (store-arg builder call-ptr 3 (const-word builder argc))
          (let ((result (emit-rt-call builder m cur-fn "rt_dyn_call" "rt_protected_dyn_call" call-ptr 4 protect)))
            (pop-sexpr-roots builder m sexpr-roots)
            result)))))

;; Fills a previously-`alloca-args`'d array, one
;; compiled argument per slot, exactly as before —
;; each `forms` element is a `(kind . arg-form)` pair
;; (`core_bridge::arg_pairs`), but no
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
;; ---- non-local exits ------------------------------------------------
;;
;; A `catch`/`unwind-protect` opens a *protected region*, and `protect` (the
;; parameter threaded beside `loop-exit`/`loop-slot`/`loop-root-base`) names
;; the region we are currently inside: its **dispatch block**, where an unwind
;; caught anywhere in the region lands. `None` means no enclosing region, and
;; then every function below emits exactly what it always emitted — not one
;; extra instruction.
;;
;; The unwind is caught in a *Rust* frame, at the call, rather than in an LLVM
;; landing pad. It has to be: stopping a Rust panic means consuming its
;; exception object, and only `std::panic::catch_unwind` can do that (see the
;; catch/throw section of `typelisp-rt` for the full reasoning). So inside a
;; region a call goes through one of the `rt_protected_*` trampolines, and what
;; follows it is `check-unwind` — an ordinary conditional branch.
;;
;; **`break`/`return` never consult `protect`.** A static exit's destination is
;; settled by the checker; it stays the `br` to the loop's exit block it has
;; always been, and the body of a `catch` stays in this same function precisely
;; so that it can.

;; The check that follows every call emitted inside a protected region: ask
;; whether that call ended in a caught unwind and, if so, branch to the
;; region's dispatch block. Leaves the builder positioned on the continuation
;; block, so the caller goes on emitting as if nothing had happened. A no-op
;; outside a region.
(defun check-unwind ((builder llvm-builder) (m llvm-module) (cur-fn llvm-function) (protect Option<llvm-basic-block>)) ()
    (match protect
      ((Some pad)
       (let* ((pending (build-call builder (get-function m "rt_unwind_pending") (alloca-args builder 0) 0))
              (cont (append-block cur-fn "unwind-check")))
         (build-cond-br builder pending pad cont)
         (position-at-end builder cont)))
      (None ())))

;; Where an unwind that this region does not stop goes next: the enclosing
;; region's dispatch block, or — when this was the outermost region in the
;; function — back to the unwinder, to keep travelling towards a `catch` in
;; some caller (compiled or interpreted).
;;
;; `rt_resume_unwind` never returns; the `build-ret` after it is unreachable
;; and exists only because LLVM requires the block to end in a terminator.
(defun emit-unwind-onward ((builder llvm-builder) (m llvm-module) (protect Option<llvm-basic-block>)) ()
    (match protect
      ((Some outer) (build-br builder outer))
      (None
       (let ((ignored (build-call builder (get-function m "rt_resume_unwind") (alloca-args builder 0) 0)))
         (build-ret builder (const-word builder 0))))))

;; Leaves an `unwind-protect`'s cleanup on the *static* exit path — the one a
;; `break`/`return` took — once that cleanup has run.
;;
;; The destination was known at check time, so there is nothing to dispatch on:
;; either another `unwind-protect` encloses this one inside the same loop, in
;; which case its cleanup has to run too and we branch to it, or this was the
;; outermost one and control goes to the loop's exit block. Only that last step
;; truncates to `loop-root-base` — each cleanup block already truncated to its
;; own entry depth on the way in, and those are all at least as deep.
;;
;; Deliberately separate from `emit-unwind-onward`: that one walks the *dynamic*
;; chain, where the next stop depends on a tag no compiler can read. Sharing a
;; walker between the two would be the very conflation this design avoids.
(defun emit-static-exit-onward ((builder llvm-builder) (m llvm-module) (loop-exit Option<llvm-basic-block>) (loop-root-base Option<llvm-value>) (exit-cleanup Option<llvm-basic-block>)) ()
    (match exit-cleanup
      ((Some outer) (build-br builder outer))
      (None
       (match loop-exit
         ((Some eb)
          (match loop-root-base
            ((Some rb)
             (let ((targs (alloca-args builder 1)))
               (store-arg builder targs 0 rb)
               (let ((ignored (build-call builder (get-function m "rt_truncate_sexpr_roots") targs 1))) ())
               (build-br builder eb)))
            (None (panic "emit-static-exit-onward: not inside a loop"))))
         (None (panic "emit-static-exit-onward: not inside a loop"))))))

;; A direct call to a statically-known function.
;;
;; Inside a region the target is *handed to* `rt_protected_call` as a value
;; (`build-fn-address`) instead of being the callee of the call instruction,
;; because the frame that catches has to be the one making the call.
(defun emit-direct-call ((builder llvm-builder) (m llvm-module) (cur-fn llvm-function) (target llvm-function) (args-ptr llvm-value) (argc i32) (protect Option<llvm-basic-block>)) llvm-value
    (match protect
      ((Some pad)
       (let ((shim (alloca-args builder 3)))
         (store-arg builder shim 0 (build-fn-address builder target))
         (store-arg builder shim 1 (build-ptr-to-int builder args-ptr))
         (store-arg builder shim 2 (const-word builder argc))
         (let ((result (build-call builder (get-function m "rt_protected_call") shim 3)))
           (check-unwind builder m cur-fn protect)
           result)))
      (None (build-call builder target args-ptr argc))))

;; [`emit-direct-call`] for the extended, captures-carrying ABI a `labels`
;; sibling is declared under (`add-function-with-env`).
(defun emit-direct-call-with-env ((builder llvm-builder) (m llvm-module) (cur-fn llvm-function) (target llvm-function) (args-ptr llvm-value) (argc i32) (env-ptr llvm-value) (env-len i32) (protect Option<llvm-basic-block>)) llvm-value
    (match protect
      ((Some pad)
       (let ((shim (alloca-args builder 5)))
         (store-arg builder shim 0 (build-fn-address builder target))
         (store-arg builder shim 1 (build-ptr-to-int builder args-ptr))
         (store-arg builder shim 2 (const-word builder argc))
         (store-arg builder shim 3 (build-ptr-to-int builder env-ptr))
         (store-arg builder shim 4 (const-word builder env-len))
         (let ((result (build-call builder (get-function m "rt_protected_call_env") shim 5)))
           (check-unwind builder m cur-fn protect)
           result)))
      (None (build-call-with-env builder target args-ptr argc env-ptr env-len))))

;; Applying a function *value*. The protected branch builds `rt_apply_any`'s
;; own three-slot argument array by hand — the same one `build-closure-apply`
;; builds internally — so the call can go to the protected entry point
;; instead. This is the case a throw raised by an *interpreted* callee comes
;; back through.
(defun emit-closure-apply ((builder llvm-builder) (m llvm-module) (cur-fn llvm-function) (closure llvm-value) (args-ptr llvm-value) (argc i32) (protect Option<llvm-basic-block>)) llvm-value
    (match protect
      ((Some pad)
       (let ((shim (alloca-args builder 3)))
         (store-arg builder shim 0 closure)
         (store-arg builder shim 1 (build-ptr-to-int builder args-ptr))
         (store-arg builder shim 2 (const-word builder argc))
         (let ((result (build-call builder (get-function m "rt_protected_apply_any") shim 3)))
           (check-unwind builder m cur-fn protect)
           result)))
      (None (build-closure-apply builder m closure args-ptr argc))))

;; A call to a runtime entry point that may unwind, where the protected form
;; takes the very same arguments — `rt_dyn_call`, `rt_panic`, `rt_throw`.
(defun emit-rt-call ((builder llvm-builder) (m llvm-module) (cur-fn llvm-function) (plain string) (guarded string) (args-ptr llvm-value) (argc i32) (protect Option<llvm-basic-block>)) llvm-value
    (match protect
      ((Some pad)
       (let ((result (build-call builder (get-function m guarded) args-ptr argc)))
         (check-unwind builder m cur-fn protect)
         result))
      (None (build-call builder (get-function m plain) args-ptr argc))))

;; that reclaims it before the call ever happens) —
;; the call-argument counterpart of
;; `compile-construct-sexpr`'s own `Cons`-field fix.
;; Returns how many such pushes it made, so the
;; caller (`compile-apply`/`compile-call`/
;; `compile-apply-indirect`) knows how many
;; `pop-sexpr-root` calls to make once the call these
;; roots were protecting is done.
(defun compile-call-args ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (args-ptr llvm-value) (forms Option<Sexpr>) (idx i32))i32
    (if (sexpr-consp forms)
        (let ((arg-pair (sexpr-car forms)) (rest (sexpr-cdr forms)))
         (let* ((kind (sexpr-i32 (sexpr-car arg-pair)))
                (form (sexpr-cdr arg-pair))
                (v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup form)))
           (store-arg builder args-ptr idx v)
         (if (eq kind 2)
             (let ((ignored (push-sexpr-root builder m v)))
               (+ 1 (compile-call-args m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr rest (+ idx 1))))
             (compile-call-args m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr rest (+ idx 1)))))
        0))

;; `(apply name (is-fn . arg-form)...)` — a direct
;; call to a name `core_bridge::translate_apply`
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
(defun compile-apply ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((nm (sexpr-str (sexpr-car (sexpr-cdr e))))
           (arg-forms (sexpr-cdr (sexpr-cdr e)))
           (argc (sexpr-list-length arg-forms))
           (args-ptr (alloca-args builder argc))
           (sexpr-roots (compile-call-args m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr arg-forms 0)))
      (match (get fn-env nm)
        ((Some target)
         (let ((env-len (sexpr-list-length captured)))
           (if (eq env-len 0)
               (let ((result (emit-direct-call builder m cur-fn target args-ptr argc protect)))
                 (pop-sexpr-roots builder m sexpr-roots)
                 result)
               (let ((env-ptr (alloca-args builder env-len)))
                 (compile-env-args m fn-name builder env fn-env captured env-ptr captured 0)
                 (let ((result (emit-direct-call-with-env builder m cur-fn target args-ptr argc env-ptr env-len protect)))
                   (pop-sexpr-roots builder m sexpr-roots)
                   result)))))
        (None (panic (append "compile-apply: no direct-callable function named " nm))))))

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
;; `core_bridge::translate_call`'s doc comment), so
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
(defun compile-call ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    ;; The name is used exactly as the node carries it. A free builtin with no
    ;; typelisp body (`sexpr-car`, `gensym`, `symbol->string`, ...) already
    ;; arrives as its `crate::compile::runtime` shim name — `symbols::
    ;; callee_symbol_name` decides that, on the same side of the boundary that
    ;; decides whether to mangle. This used to re-derive it from a nested `if`
    ;; chain over the raw names, which meant two lists that had to agree about
    ;; every builtin; `get-function` aborting the process on a name it cannot
    ;; find is what disagreement cost.
    (let* ((nm (sexpr-str (sexpr-car (sexpr-cdr e))))
           (arg-forms (sexpr-cdr (sexpr-cdr e)))
           (argc (sexpr-list-length arg-forms))
           (args-ptr (alloca-args builder argc))
           (sexpr-roots (compile-call-args m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr arg-forms 0))
           (result (emit-direct-call builder m cur-fn (get-function m nm) args-ptr argc protect)))
      (pop-sexpr-roots builder m sexpr-roots)
      result))

;; `(apply-indirect callee-form (is-fn . arg-form)...)`
;; — `Expr::Apply`, labels/closures Stage 4, the
;; general indirect-dispatch case
;; (`core_bridge::translate_apply`'s doc
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
(defun compile-apply-indirect ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((callee-form (sexpr-car (sexpr-cdr e)))
           (closure (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup callee-form))
           (arg-forms (sexpr-cdr (sexpr-cdr e)))
           (argc (sexpr-list-length arg-forms))
           (args-ptr (alloca-args builder argc))
           (sexpr-roots (compile-call-args m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr arg-forms 0))
           (result (emit-closure-apply builder m cur-fn closure args-ptr argc protect)))
      (pop-sexpr-roots builder m sexpr-roots)
      result))

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
(defun compile-if-branch ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (is-fn bool) (form Option<Sexpr>))llvm-value
    (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup form))

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
(defun compile-if ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((is-fn (sexpr-bool (sexpr-car (sexpr-cdr e))))
           (cond-form (sexpr-car (sexpr-cdr (sexpr-cdr e))))
           (then-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
           (else-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
           (cond-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup cond-form))
           (then-block (append-block cur-fn "if-then"))
           (else-block (append-block cur-fn "if-else"))
           (merge-block (append-block cur-fn "if-merge"))
           (slot (alloca-args builder 1)))
      (build-cond-br builder cond-v then-block else-block)
      (position-at-end builder then-block)
      (let ((then-v (compile-if-branch m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup is-fn then-form)))
        (if (block-terminated? builder)
            ()
            (let ((ignored (store-arg builder slot 0 then-v)))
              (build-br builder merge-block))))
      (position-at-end builder else-block)
      (let ((else-v (compile-if-branch m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup is-fn else-form)))
        (if (block-terminated? builder)
            ()
            (let ((ignored (store-arg builder slot 0 else-v)))
              (build-br builder merge-block))))
      (position-at-end builder merge-block)
      (load-raw builder slot 0)))

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
(defun compile-let-values ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (bindings Option<Sexpr>) (acc Scope<llvm-value>))()
    (if (sexpr-consp bindings)
        (let ((pair (sexpr-car bindings)) (rest (sexpr-cdr bindings)))
         (let* ((nm (sexpr-sym-name (sexpr-car (sexpr-car pair))))
                (form (sexpr-cdr pair))
                (v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup form)))
           (set acc nm v)
         (compile-let-values m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup rest acc)))
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
(defun compile-let-body ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (forms Option<Sexpr>))llvm-value
    (if (sexpr-consp forms)
        (let ((form (sexpr-car forms)) (rest (sexpr-cdr forms)))
         (let ((v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup form)))
         (if (block-terminated? builder)
             v
             (if (sexpr-consp rest)
                 (compile-let-body m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup rest)
                 v))))
        (compile-unit m fn-name builder)))

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
(defun compile-let ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((bindings (sexpr-car (sexpr-cdr e)))
           (body-forms (sexpr-cdr (sexpr-cdr e)))
           (acc (new-acc-table)))
      (compile-let-values m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup bindings acc)
      (push-frame env)
      (bind-let-values builder m env bindings acc)
      (let ((result (compile-let-body m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup body-forms)))
        (if (block-terminated? builder)
            ()
            (unroot-let-sexpr-values builder m bindings))
        (pop-frame env)
        result)))

;; `(lambda name ((captured . kind)...) ((param . kind)...) body)`
;; — `Expr::Lambda` (a standalone escaping value) or a
;; synthesized `Expr::FnRef` forwarding wrapper
;; (`core_bridge::translate_fnref`) — labels/closures
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
;; encloses it — see `core_bridge::translate_lambda`'s
;; doc comment), R1-pushing GC roots for its own
;; params/captures on entry and R2-popping them on
;; exit exactly like `compile-function`'s own
;; top-level body and each `labels` sibling's body do,
;; then builds an env array from `lcaptured`'s
;; *current* values in the *outer* `env`
;; (`compile-escaping-env-args`) and wraps the whole
;; thing into a `BoxedObj::CompiledClosure`
;; (`build-make-closure`, with `compute-sexpr-mask-part`'s
;; bitmask so the GC mark phase knows which of its own
;; captured slots are tagged values to trace) — the
;; value this whole node evaluates to.
(defun compile-lambda ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (e Option<Sexpr>))llvm-value
    (let* ((lname (sexpr-str (sexpr-car (sexpr-cdr e))))
           (lcaptured (sexpr-car (sexpr-cdr (sexpr-cdr e))))
           (lparams (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
           (lbody (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
           (nested-fn (add-function-with-env m lname)))
      (let* ((nested-block (append-block nested-fn "entry"))
             (nested-builder (llvm-builder::create)))
        (position-at-end nested-builder nested-block)
        (let ((nested-env (new-env)))
          ;; `bind-captures` before `bind-params`
          ;; so a parameter shadows any captured
          ;; name it collides with — see
          ;; `compile-labels-bodies`'s matching
          ;; comment for why this ordering is
          ;; correct and root-neutral.
          ;; No `retain-bindings` call for
          ;; `lcaptured` (unlike `lparams`):
          ;; every entry a captured-name list can
          ;; ever hold is `kind >= 10` now
          ;; (Stage 4), and `bind-captures`
          ;; itself already pushes a permanent
          ;; root for each one — a
          ;; `retain-bindings` pass here would
          ;; only ever match its now-unused
          ;; `kind = 2` case, a pure no-op.
          (bind-captures nested-env nested-builder m nested-fn lcaptured 0)
          (bind-params nested-env nested-builder m nested-fn lparams 0)
          (retain-bindings nested-builder m nested-env lparams)
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
          (let ((v (compile-value m fn-name nested-builder nested-env (new-fn-env) lcaptured nested-fn (Option::none) (Option::none) (Option::none) (Option::none) (Option::none) lbody)))
            (release-bindings nested-builder m nested-env lparams)
            (release-bindings nested-builder m nested-env lcaptured)
            (build-ret nested-builder v))))
      (let* ((env-len (sexpr-list-length lcaptured))
             (env-ptr (alloca-args builder env-len)))
        (compile-escaping-env-args m fn-name builder env fn-env captured env-ptr lcaptured 0)
        (build-make-closure builder m nested-fn env-ptr env-len (compute-sexpr-mask-part lcaptured 0 0) (compute-sexpr-mask-part lcaptured 0 32)))))

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
(defun declare-labels-siblings ((m llvm-module) (fn-name string) (inner-fn-env Scope<llvm-function>) (captured Option<Sexpr>) (defs Option<Sexpr>))()
    (if (sexpr-consp defs)
        (let ((def (sexpr-car defs)) (rest (sexpr-cdr defs)))
         (let* ((nm (sexpr-str (sexpr-car def)))
                (mangled (append fn-name (append "$" nm))))
           (if (eq (sexpr-list-length captured) 0)
             (set inner-fn-env nm (add-function m mangled))
             (set inner-fn-env nm (add-function-with-env m mangled)))
         (declare-labels-siblings m fn-name inner-fn-env captured rest)))
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
(defun compile-labels-bodies ((m llvm-module) (fn-name string) (inner-fn-env Scope<llvm-function>) (captured Option<Sexpr>) (defs Option<Sexpr>))()
    (if (sexpr-consp defs)
        (let ((def (sexpr-car defs)) (rest (sexpr-cdr defs)))
         (let* ((nm (sexpr-str (sexpr-car def)))
                (param-syms (sexpr-car (sexpr-cdr def)))
                (def-body (sexpr-car (sexpr-cdr (sexpr-cdr def)))))
           (match (get inner-fn-env nm)
           ((Some sib-fn)
            (let* ((sib-block (append-block sib-fn "entry"))
                   (sib-builder (llvm-builder::create)))
              (position-at-end sib-builder sib-block)
              (let ((sib-env (new-env)))
                ;; `bind-captures` runs *before*
                ;; `bind-params` so a parameter
                ;; shadows any block-captured name
                ;; it collides with: both bind into
                ;; `sib-env` by name, and the last
                ;; write wins, so the parameter must
                ;; be second (lexical shadowing —
                ;; e.g. `resolve-value`'s own `name`
                ;; parameter must shadow the
                ;; captured enclosing `name` that
                ;; `declare-labels-siblings`
                ;; genuinely needs; observed
                ;; self-hosting the island). Root
                ;; bookkeeping is unaffected:
                ;; `release-bindings` is count-based
                ;; (it pops the top of the root
                ;; stack N times, never by name), so
                ;; the two lists' pushes and pops
                ;; stay balanced regardless of bind
                ;; order. No `retain-bindings` call
                ;; for `captured` — every entry is
                ;; `kind >= 10` now, and
                ;; `bind-captures` already roots each
                ;; one itself.
                (bind-captures sib-env sib-builder m sib-fn captured 0)
                (bind-params sib-env sib-builder m sib-fn param-syms 0)
                (retain-bindings sib-builder m sib-env param-syms)
                ;; `loop`/`break`/`return`/`setf`:
                ;; a `labels` sibling's own body is
                ;; a new function boundary too
                ;; (`Checker::check_labels` resets
                ;; the loop stack per def, exactly
                ;; like `check_lambda` — see
                ;; `compile-lambda`'s matching
                ;; comment), so no enclosing loop
                ;; here either.
                (let* ((sib-fn-env (clone-frames inner-fn-env))
                       (v (compile-value m fn-name sib-builder sib-env sib-fn-env captured sib-fn (Option::none) (Option::none) (Option::none) (Option::none) (Option::none) def-body)))
                  (release-bindings sib-builder m sib-env param-syms)
                  (release-bindings sib-builder m sib-env captured)
                  (build-ret sib-builder v)
                  (compile-labels-bodies m fn-name inner-fn-env captured rest)))))
           (None (panic (append "compile-labels-bodies: missing declaration for " nm))))))
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
(defun compile-labels ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((inner-captured (sexpr-car (sexpr-cdr e)))
           (defs (sexpr-car (sexpr-cdr (sexpr-cdr e))))
           (trailing (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
      (push-frame fn-env)
      (declare-labels-siblings m fn-name fn-env inner-captured defs)
      (compile-labels-bodies m fn-name fn-env inner-captured defs)
      ;; The trailing body is *not* a new function
      ;; boundary (`Checker::check_labels` checks
      ;; `args[1..]` against the *unmodified*
      ;; loop stack — see this module's doc
      ;; comment), so `loop-exit`/`loop-slot` are
      ;; forwarded unchanged here, unlike each
      ;; def's own body just above.
      (let ((result (compile-value m fn-name builder env fn-env inner-captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup trailing)))
        (pop-frame fn-env)
        result)))

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
(defun compile-loop ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((body-forms (sexpr-cdr e))
           (loop-block (append-block cur-fn "loop-body"))
           (exit-block (append-block cur-fn "loop-exit"))
           (slot (alloca-args builder 1))
           (root-base (build-call builder (get-function m "rt_root_count") (alloca-args builder 0) 0)))
      (build-br builder loop-block)
      (position-at-end builder loop-block)
      (compile-loop-body m fn-name builder env fn-env captured cur-fn (Option::some exit-block) (Option::some slot) (Option::some root-base) protect (Option::none) body-forms)
      (if (block-terminated? builder)
          ()
          (build-br builder loop-block))
      (position-at-end builder exit-block)
      (load-raw builder slot 0)))

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
(defun compile-loop-body ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (forms Option<Sexpr>))()
    (if (sexpr-consp forms)
        (let ((form (sexpr-car forms)) (rest (sexpr-cdr forms)))
         (let ((ignored (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup form)))
         (if (block-terminated? builder)
             ()
             (compile-loop-body m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup rest))))
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
(defun compile-break ((m llvm-module) (fn-name string) (builder llvm-builder) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (exit-cleanup Option<llvm-basic-block>))llvm-value
    (match loop-exit
      ((Some eb)
       (match loop-slot
         ((Some slot)
          (match loop-root-base
            ((Some rb)
             (let ((zero (const-word builder 0)))
               (store-arg builder slot 0 zero)
               (match exit-cleanup
                 ((Some xe) (build-br builder xe))
                 (None
                  (let ((truncate-args (alloca-args builder 1)))
                    (store-arg builder truncate-args 0 rb)
                    (let ((ignored (build-call builder (get-function m "rt_truncate_sexpr_roots") truncate-args 1))) ())
                    (build-br builder eb))))
               zero))
            (None (panic "compile-break: not inside a loop"))))
         (None (panic "compile-break: not inside a loop"))))
      (None (panic "compile-break: not inside a loop"))))

;; `(return is-fn value-form)` — `Expr::Return`,
;; including the implicit `Unit` `core_bridge` already
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
(defun compile-return ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((is-fn (sexpr-bool (sexpr-car (sexpr-cdr e))))
           (value-form (sexpr-car (sexpr-cdr (sexpr-cdr e))))
           (v (compile-if-branch m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup is-fn value-form)))
      (match loop-exit
        ((Some eb)
         (match loop-slot
           ((Some slot)
            (match loop-root-base
              ((Some rb)
               (let ((ignored0 (store-arg builder slot 0 v)))
                 (match exit-cleanup
                   ((Some xe) (build-br builder xe))
                   (None
                    (let ((truncate-args (alloca-args builder 1)))
                      (store-arg builder truncate-args 0 rb)
                      (let ((ignored (build-call builder (get-function m "rt_truncate_sexpr_roots") truncate-args 1))) ())
                      (build-br builder eb))))
                 v))
              (None (panic "compile-return: not inside a loop"))))
           (None (panic "compile-return: not inside a loop"))))
        (None (panic "compile-return: not inside a loop")))))

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
;; `core_bridge::translate_set`'s doc comment): `kind =
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
(defun compile-set ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((nm (sexpr-str (sexpr-car (sexpr-cdr e))))
           (kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
           (is-fn (eq kind 1))
           (value-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
           (v (compile-if-branch m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup is-fn value-form)))
      (match (get env nm)
        ((Some slot)
         (store-arg builder slot 0 v)
         (if (eq kind 2)
             (let* ((root-idx (load-raw builder slot 1))
                    (args-ptr (alloca-args builder 2)))
               (store-arg builder args-ptr 0 root-idx)
               (store-arg builder args-ptr 1 v)
               (let ((ignored (build-call builder (get-function m "rt_set_sexpr_root") args-ptr 2))) ()))
             ())
         v)
        (None (panic (append "compile-set: unbound variable " nm))))))

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
(defun compile-cellset ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((nm (sexpr-str (sexpr-car (sexpr-cdr e))))
           (kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
           (value-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
           (v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup value-form)))
      (match (get env nm)
        ((Some slot)
         (let* ((cell-ref (load-raw builder slot 0))
                (tagged (compile-tag-struct-field builder m v kind))
                (args-ptr (alloca-args builder 2)))
           (store-arg builder args-ptr 0 cell-ref)
           (store-arg builder args-ptr 1 tagged)
           (let ((ignored (build-call builder (get-function m "rt_cell_set") args-ptr 2)))
             v)))
        (None (panic (append "compile-cellset: unbound variable " nm))))))

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
(defun compile-pattern-test ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (v llvm-value) (pat Option<Sexpr>) (fail-block llvm-basic-block))()
    (case (sexpr-car pat)
      ;; Matches anything and binds nothing, so there is no guard to emit.
      (pat-wild ())
      ;; `(pat-empty)` -- the empty list. The same word comparison a
      ;; `pat-ctor` on `Sexpr`'s variant 0 emits, under a node that does not
      ;; name a variant: the empty list outlives `Sexpr`'s `nil`.
      (pat-empty
       (compile-pattern-guard builder cur-fn (compile-sexpr-tag-test builder m v 0) fail-block))
      ;; `(pat-nonempty P)` -- `Option<Sexpr>`'s `(some P)`. The niche makes
      ;; the unwrapped value the same word, so this rejects the empty list
      ;; and then applies `P` to that same `v`.
      ;;
      ;; One comparison, not the negation of `compile-sexpr-tag-test`'s
      ;; two-part nil test: the empty list is a single word. `Value::Empty`
      ;; encodes as `(IMMEDIATE_NIL << TAG_BITS) | TAG_IMMEDIATE` =
      ;; `(0 << 3) | 6` = 6 (`typelisp-abi`), so "tag is 6 and payload is 0"
      ;; and "the word is 6" are the same test, and this is its opposite.
      (pat-nonempty
       (progn
         (compile-pattern-guard builder cur-fn
           (build-icmp-ne builder v (const-word builder 6)) fail-block)
         (compile-pattern-test m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup v (sexpr-car (sexpr-cdr pat)) fail-block)))
      ;; Binds into a fresh slot in this arm's own frame.
      (pat-bind
       (let* ((nm (sexpr-str (sexpr-car (sexpr-cdr pat))))
              (bslot (alloca-args builder 1)))
         (store-arg builder bslot 0 v)
         (set env nm bslot)))
      ;; `(pat-lit HI LO)` -- two 32-bit halves, like `(int HI LO)` and
      ;; `(float HI LO)`. A pattern's integer is matched against an `Sexpr`,
      ;; so it already fits in the 61 bits a tagged int carries; the shape is
      ;; uniform so that "an integer crosses as two halves" has no exceptions.
      (pat-lit
       (compile-pattern-guard builder cur-fn
         (build-icmp-eq builder v
           (build-or builder
                     (build-shl builder (const-word builder (sexpr-i32 (sexpr-car (sexpr-cdr pat)))) (const-word builder 32))
                     (const-word builder (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr pat)))))))
         fail-block))
      ;; `(pat-guard NAME TEST)` -- the value pattern: bind the value under
      ;; test to NAME (exactly as `pat-bind` binds, and into this arm's own
      ;; frame, so it is discarded with it), compile TEST, and branch on the
      ;; `bool` it produces. This is what a `string`/`f64`/`symbol`/`bignum`/
      ;; `ratio` literal pattern and an explicit `(= expr)` both lower to:
      ;; their equality is a call, not a word comparison, so there is nothing
      ;; for `pat-lit` to compare. The test is an ordinary expression --
      ;; `(equals NAME ...)`, already resolved to a concrete method by the
      ;; checker -- so `compile-value` handles the whole of it, including
      ;; rooting whatever it allocates.
      (pat-guard
       (let* ((nm (sexpr-str (sexpr-car (sexpr-cdr pat))))
              (test (sexpr-car (sexpr-cdr (sexpr-cdr pat))))
              (bslot (alloca-args builder 1)))
         (store-arg builder bslot 0 v)
         (set env nm bslot)
         (compile-pattern-guard builder cur-fn
           (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup test)
           fail-block)))
      ;; `(pat-ctor VARIANT (SUB...) SCRUT-KIND (FIELD-KIND...) DOWNCAST
      ;; TYPE-NAME)`.
      (pat-ctor (compile-ctor-pattern m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup v pat fail-block))
      ;; `(pat-typetest TYPE-NAME SUB)` -- `(the Type pattern)`'s whole-value
      ;; `Sexpr` downcast (`core_bridge::translate_pattern`'s `the` arm). No
      ;; variant restriction (`-1`): it matches any variant of an enum
      ;; `Type`, the whole point of a type-only (not field-destructuring)
      ;; downcast.
      (pat-typetest
       (let* ((type-name-form (sexpr-car (sexpr-cdr pat)))
              (inner (sexpr-car (sexpr-cdr (sexpr-cdr pat)))))
         (compile-pattern-guard builder cur-fn (compile-sexpr-instance-test m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup v type-name-form -1) fail-block)
         (compile-pattern-test m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup v inner fail-block)))
      (else (panic (append "compile-pattern-test: unsupported pattern tag " (sexpr-sym-name (sexpr-car pat)))))))

;; The `pat-ctor` arm of `compile-pattern-test`, split out because its five
;; fields and three scrutinee kinds are a paragraph of their own.
;;
;; `scrut-kind`: 0 = `Sexpr` (tagged-i64 tests), 1 = sum-ADT box (tests and
;; extracts by slot), 2 = boxed struct (`defstruct`/`Vector` -- single
;; variant, so no tag test at all, only field extraction via `field-kinds`).
;;
;; `downcast`/`type-name-form` (Sexpr-user-ADT design plan §4,
;; `core_bridge::translate_pattern`'s trailing two fields): a Sexpr-downcast
;; `pat-ctor` -- the checker discovered this struct/enum *inside* a
;; heterogeneous `Sexpr`, not from the scrutinee's own static type -- needs an
;; instance test the ordinary case never did. `downcast` false is a total
;; no-op, exactly the pre-existing scrut-kind 1/2 behavior unchanged.
(defun compile-ctor-pattern ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (v llvm-value) (pat Option<Sexpr>) (fail-block llvm-basic-block))()
  (let* ((variant (sexpr-i32 (sexpr-car (sexpr-cdr pat))))
         (subpats (sexpr-car (sexpr-cdr (sexpr-cdr pat))))
         (scrut-kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr pat))))))
         (field-kinds (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr pat))))))
         (downcast (sexpr-bool (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr pat))))))))
         (type-name-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr pat)))))))))
    (case scrut-kind
      ;; A boxed struct has one variant, so only a downcast tests
      ;; anything at all.
      (2 (if downcast
             (compile-pattern-guard builder cur-fn (compile-sexpr-instance-test m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup v type-name-form -1) fail-block)
             ()))
      ;; A sum-ADT box: the variant slot, or the instance test when
      ;; the type itself was discovered rather than known.
      (1 (if downcast
             (compile-pattern-guard builder cur-fn (compile-sexpr-instance-test m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup v type-name-form variant) fail-block)
             (compile-pattern-guard builder cur-fn (compile-box-tag-test builder m v variant) fail-block)))
      ;; `0` -- a tagged `Sexpr`.
      (else
       (compile-pattern-guard builder cur-fn (compile-sexpr-tag-test builder m v variant) fail-block)))
    (compile-ctor-subpatterns m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup v scrut-kind variant field-kinds subpats 0 fail-block)))

;; Compiles `type-name-form` (a compile-time-known
;; `(str (int c)...)` literal, `str_literal_form`'s
;; shape -- see `translate_construct`'s own
;; `type-name-form` doc comment) into a real runtime
;; `Sexpr::Str`, then calls `rt_sexpr_instance_test`
;; to test scrutinee `v` against it (and, when
;; `variant >= 0`, that variant too -- `-1` skips the
;; check, for a struct downcast or a `(the T p)`
;; whole-enum bind). The shared guard both a
;; downcast `pat-ctor` and a `pat-typetest` use.
(defun compile-sexpr-instance-test ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (v llvm-value) (type-name-form Option<Sexpr>) (variant i32))llvm-value
    (let* ((name-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup type-name-form))
           (args-ptr (alloca-args builder 3)))
      (store-arg builder args-ptr 0 v)
      (store-arg builder args-ptr 1 name-v)
      (store-arg builder args-ptr 2 (const-word builder variant))
      (build-icmp-eq builder (build-call builder (get-function m "rt_sexpr_instance_test") args-ptr 3) (const-word builder 1))))

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
(defun compile-ctor-subpatterns ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (v llvm-value) (scrut-kind i32) (variant i32) (field-kinds Option<Sexpr>) (subpats Option<Sexpr>) (idx i32) (fail-block llvm-basic-block))()
    (if (sexpr-consp subpats)
        (let ((p (sexpr-car subpats)) (rest (sexpr-cdr subpats)))
         (let ((rest-kinds (if (eq scrut-kind 0) field-kinds (sexpr-cdr field-kinds))))
          (if (equal (sexpr-car p) (quote pat-wild))
           (compile-ctor-subpatterns m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup v scrut-kind variant rest-kinds rest (+ idx 1) fail-block)
           (let ((field-v (case scrut-kind
                            ;; A boxed struct and a sum-ADT box both carry a
                            ;; per-field kind; a tagged `Sexpr` (`0`) reads its
                            ;; field shape off the variant instead.
                            (2 (compile-struct-field builder m v (sexpr-i32 (sexpr-car field-kinds)) idx))
                            (1 (compile-box-field builder m v (sexpr-i32 (sexpr-car field-kinds)) idx))
                            (else (compile-sexpr-field builder m v variant idx)))))
             (compile-pattern-test m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup field-v p fail-block)
             (compile-ctor-subpatterns m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup v scrut-kind variant rest-kinds rest (+ idx 1) fail-block)))))
        ()))

;; `(match is-fn scrutinee-form ((pattern-form .
;; body-form)...) scrut-kind)` -- `Expr::Match`
;; (`core_bridge::translate_match`; any scrutinee type
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
;; kinds root/unroot identically. What is *not* a heap
;; value roots not at all: a scrutinee can now be a
;; scalar (`(match n (1 ...) (_ ...))`), and the
;; node's trailing root-kind field says which — see
;; the comment at the read itself.
(defun compile-match ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((is-fn (sexpr-bool (sexpr-car (sexpr-cdr e))))
           (scrut-form (sexpr-car (sexpr-cdr (sexpr-cdr e))))
           (arms (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
           (root-kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))
           (scrut-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup scrut-form))
           (ignored (if (eq root-kind 2) (push-sexpr-root builder m scrut-v) ()))
           (merge-block (append-block cur-fn "match-merge"))
           (slot (alloca-args builder 1)))
      (compile-match-arms m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup is-fn scrut-v slot merge-block arms)
      (position-at-end builder merge-block)
      (let* ((result (load-raw builder slot 0))
             (ignored2 (if (eq root-kind 2) (pop-sexpr-root builder m) ())))
        result)))

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
(defun compile-match-arms ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (is-fn bool) (scrut-v llvm-value) (slot llvm-value) (merge-block llvm-basic-block) (arms Option<Sexpr>))()
    (if (sexpr-consp arms)
        (let ((arm (sexpr-car arms)) (rest (sexpr-cdr arms)))
         (let* ((pat (sexpr-car arm))
                (body-form (sexpr-cdr arm)))
           (push-frame env)
         (let ((next-block (append-block cur-fn "match-next")))
           (compile-pattern-test m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup scrut-v pat next-block)
           (let ((body-v (compile-if-branch m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup is-fn body-form)))
             (pop-frame env)
             (if (block-terminated? builder)
                 ()
                 (let ((ignored (store-arg builder slot 0 body-v)))
                   (build-br builder merge-block))))
           (position-at-end builder next-block)
           (compile-match-arms m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup is-fn scrut-v slot merge-block rest))))
        (let* ((args-ptr (alloca-args builder 0))
               (fallback (build-call builder (get-function m "rt_match_fail") args-ptr 0)))
          (store-arg builder slot 0 fallback)
           (build-br builder merge-block))))

;; `(construct is-sexpr mutable type-name-str
;; variant-i64 arg-form...)` (Stage 6 of the
;; Sexpr-representation plan, extended by Stage 3 of
;; the Sexpr/RtValue unification plan —
;; `docs/implementation-log.md` — with `mutable`/
;; `type-name-str`) — `is-sexpr`/`mutable`
;; (`Repr::Sexpr`/`translate_construct`'s
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
;; `core_bridge::quoted_form`'s doc comment) are
;; peeled off *here*, before delegating to
;; `compile-construct-sexpr`, rather than folded into
;; that function's own already-deep dispatch chain —
;; adding even one more arm there was enough to
;; overflow an unrelated test's stack (`scripts/
;; test-serial.sh`'s `RUST_MIN_STACK` comment records
;; the incident), since `Checker::check_if`/
;; `Interp::eval` never loopified `if`-chain recursion
;; the way `compile-value`'s own dispatch did.
(defun compile-construct ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((is-sexpr (sexpr-bool (sexpr-car (sexpr-cdr e))))
           (mutable (sexpr-bool (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
           (type-name-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
           (variant (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))
           (arg-forms (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))
      (case variant
        (100 (compile-construct-sym m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup arg-forms))
        (101 (compile-construct-path m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup arg-forms))
        (102 (compile-construct-wk-sym m fn-name builder arg-forms))
        ;; Every other variant index is a real one, and which of the
        ;; three constructors it names is decided by the node's two
        ;; flags rather than by the index.
        (else
         (if is-sexpr
             (compile-construct-sexpr m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup variant arg-forms)
             (if mutable
                 (compile-construct-boxed-struct m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup type-name-form arg-forms)
                 (compile-construct-box m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup type-name-form variant arg-forms)))))))

;; `(construct true false empty 100 name-form seg-form...)`
;; — a quoted symbol literal (`core_bridge::sym_form`).
;; Every form is an ordinary `(str ...)` node
;; (`str_literal_form`); compiling one yields a fully
;; tagged `Sexpr::Str`. The first is the symbol's name
;; and the rest are the segments of the module that
;; **owns** it, root being none at all: two modules that
;; both write `foo` own two different symbols, so a bare
;; name would intern into whichever module the runtime
;; defaulted to. `rt_intern_symbol` takes them in that
;; order. No GC-root protection is needed — an interned
;; symbol is permanent, unlike the `Str`s that briefly
;; hold the spelling.
(defun compile-construct-sym ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (arg-forms Option<Sexpr>))llvm-value
    (let* ((n (sexpr-list-length arg-forms))
           (args-ptr (alloca-args builder n)))
      (compile-construct-sym-args m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr arg-forms 0)
      (build-call builder (get-function m "rt_intern_symbol") args-ptr n)))

;; Fills a `compile-construct-sym`-allocated array, one
;; compiled `(str ...)` per slot — the symbol analogue of
;; `compile-construct-path-segs`, which differs only in
;; interning each of its own slots on the way in.
(defun compile-construct-sym-args ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (args-ptr llvm-value) (forms Option<Sexpr>) (idx i32))()
    (if (sexpr-consp forms)
        (let ((form (sexpr-car forms)) (rest (sexpr-cdr forms)))
          (store-arg builder args-ptr idx (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup form))
          (compile-construct-sym-args m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr rest (+ idx 1)))
        ()))

;; `(construct true false empty 102 INDEX)` — a quoted
;; symbol from the system module, named by its
;; `BUILTIN_SYMBOLS` index rather than by its spelling
;; (`core_bridge::sym_form`). `INDEX` is a plain integer
;; field read here at compile time, not a form to
;; compile: every process builds the vocabulary from the
;; same list in the same order, so the index means the
;; same symbol everywhere. One `rt_wk_symbol` call, one
;; array index — no string is built and nothing is
;; interned.
(defun compile-construct-wk-sym ((m llvm-module) (fn-name string) (builder llvm-builder) (arg-forms Option<Sexpr>))llvm-value
    (let ((args-ptr (alloca-args builder 1)))
      (store-arg builder args-ptr 0 (const-word builder (sexpr-i32 (sexpr-car arg-forms))))
      (build-call builder (get-function m "rt_wk_symbol") args-ptr 1)))

;; `(construct true false empty 101 seg-form...)` — a
;; quoted `::`-path literal (`core_bridge::quoted_form`'s
;; `Path` arm, `QUOTE_PATH_MARKER`), one `(str ...)`
;; node per segment. `compile-construct-path-segs`
;; interns each segment (`rt_intern_symbol`, same as
;; `compile-construct-sym`) into a fresh `args-ptr`
;; array, then `rt_intern_path` combines them into the
;; final tagged `Sexpr::Path`.
(defun compile-construct-path ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (arg-forms Option<Sexpr>))llvm-value
    (let* ((n (sexpr-list-length arg-forms))
           (args-ptr (alloca-args builder n)))
      (compile-construct-path-segs m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr arg-forms 0)
      (build-call builder (get-function m "rt_intern_path") args-ptr n)))

;; Fills a `compile-construct-path`-allocated array,
;; one interned segment `Sexpr::Symbol` per slot —
;; the path-literal analogue of `store-str-chars`.
(defun compile-construct-path-segs ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (args-ptr llvm-value) (forms Option<Sexpr>) (idx i32))()
    (if (sexpr-consp forms)
        (let ((form (sexpr-car forms)) (rest (sexpr-cdr forms)))
          (let* ((name-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup form))
                 (seg-args-ptr (alloca-args builder 1)))
            (store-arg builder seg-args-ptr 0 name-v)
            (let ((sym-v (build-call builder (get-function m "rt_intern_symbol") seg-args-ptr 1)))
              (store-arg builder args-ptr idx sym-v)
              (compile-construct-path-segs m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr rest (+ idx 1)))))
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
;; `Repr::field_kind`
;; (`compile-construct-boxed-struct-fields`) before
;; being handed to `rt_struct_new`, which expects
;; every argument after the type name to already be a
;; properly tagged `Sexpr` (that function's own
;; `decode()` call on each). The freshly built struct
;; is `push-permanent-sexpr-root`ed immediately —
;; the same accepted-leak treatment
;; `compile-construct-box-fields` already gives a
;; general-ADT box's own `Sexpr`-typed fields, just
;; applied to the struct itself rather than to one of
;; its fields, so this sidesteps tracking the struct's
;; own binding scope at all.
;;
;; This root used to be the *only* thing keeping such a
;; struct alive: `binding_kind` classified a struct
;; binding `KIND_PLAIN`, on the argument that a
;; permanent root already keeps every boxed struct alive
;; from birth and per-binding push/pop would be
;; redundant. True, but it made correctness rest on a
;; leak — a permanent root is never popped, by design
;; (`rt_push_permanent_sexpr_root`'s doc comment), so
;; whoever gives compiled boxes real lifetimes would
;; have removed the only thing standing between an
;; unrooted binding and a collection. `Repr::class` now
;; derives both kinds from one judgement, so a struct
;; binding is `kind = 2` like any other reclaimable
;; value and this root is redundant rather than load-
;; bearing.
(defun compile-construct-boxed-struct ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (type-name-form Option<Sexpr>) (arg-forms Option<Sexpr>))llvm-value
    (let* ((argc (sexpr-list-length arg-forms))
           (args-ptr (alloca-args builder (+ argc 1)))
           (name-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup type-name-form)))
      (store-arg builder args-ptr 0 name-v)
      (compile-construct-boxed-struct-fields m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr arg-forms 1)
      (let ((result (build-call builder (get-function m "rt_struct_new") args-ptr (+ argc 1))))
        (push-permanent-sexpr-root builder m result)
        result)))

;; Fills a `compile-construct-boxed-struct`-allocated
;; argument array, one compiled-then-tagged field per
;; slot starting at `idx` (`1`, skipping the type-name
;; slot `rt_struct_new`'s own `args[0]` occupies) —
;; the boxed-struct analogue of
;; `compile-construct-box-fields`. Each `forms`
;; element is a `(kind . field-form)` pair
;; (`core_bridge::arg_pairs`,
;; tagged by `Repr::field_kind` rather
;; than `binding_kind`); `compile-tag-struct-field`
;; turns the field's own compiled (untagged, for a
;; scalar kind) value into the tagged `Sexpr`
;; `rt_struct_new` requires.
(defun compile-construct-boxed-struct-fields ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (args-ptr llvm-value) (forms Option<Sexpr>) (idx i32))()
    (if (sexpr-consp forms)
        (let ((field-pair (sexpr-car forms)) (rest (sexpr-cdr forms)))
         (let* ((kind (sexpr-i32 (sexpr-car field-pair)))
                (form (sexpr-cdr field-pair))
                (v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup form))
                (tagged-v (compile-tag-struct-field builder m v kind)))
           (store-arg builder args-ptr idx tagged-v)
         (compile-construct-boxed-struct-fields m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr rest (+ idx 1))))
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
;; `core_bridge::translate_construct`'s enum branch now
;; tags its fields by `struct_field_kind`
;; (`struct_field_ast_list_to_sexpr`) exactly like the
;; `mutable` branch does — every enum type compiled
;; code can ever mention is heap-repr by construction
;; (see `Repr::field_kind`'s doc comment),
;; so there is no longer a distinct "general-ADT field"
;; tagging scheme to keep separate. The freshly built
;; enum value is `push-permanent-sexpr-root`ed
;; immediately, the same accepted-forever-alive
;; treatment `compile-construct-boxed-struct` already
;; gives its own result (not a leak: a real GC-managed
;; value now, just never released early).
(defun compile-construct-box ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (type-name-form Option<Sexpr>) (variant i32) (arg-forms Option<Sexpr>))llvm-value
    (let* ((argc (sexpr-list-length arg-forms))
           (args-ptr (alloca-args builder (+ argc 2)))
           (name-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup type-name-form)))
      (store-arg builder args-ptr 0 name-v)
      (store-arg builder args-ptr 1 (const-word builder variant))
      (compile-construct-boxed-struct-fields m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup args-ptr arg-forms 2)
      (let ((result (build-call builder (get-function m "rt_data_new") args-ptr (+ argc 2))))
        (push-permanent-sexpr-root builder m result)
        result)))

;; `Sexpr`'s own variants, encoded directly as the
;; tagged `i64` `typelisp-rt`'s `encode` (and this
;; file's own `compile-sexpr-field`) already use —
;; see `registry::sexpr_def`'s variant order (`0`=nil
;; `1`=i32 `2`=f64 `3`=char `4`=bool `5`=sym `6`=str
;; `7`=cons `8`=bignum `9`=ratio `10`=path `11`=f32
;; `12`=i8 `13`=i16 `14`=u8 `15`=u16 `16`=u32), the exact inverse of
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
;; value passed straight through. The float and narrow-integer
;; fields (Sexpr/RtValue unification, Stage 0) are
;; boxed via `rt_f64_new`/`rt_f32_new` (`typelisp-rt`'s
;; `TAG_BOXED`) rather than any bit manipulation here
;; — `f64` doesn't fit alongside a 3-bit tag the way
;; `int`/`char`/`bool` do. The field's own compiled
;; value is already the raw `f64` bit pattern
;; (`f64::to_bits`), the convention `rt_f64_new`
;; expects. `sym`'s `Str` field still isn't
;; representable in compiled code yet (a `Sym`'s tag
;; payload is an interned symbol's address, a separate
;; gap from `str`'s
;; own — see `compile-sexpr-field`'s doc comment), so
;; constructing one still panics clearly.
(defun compile-construct-sexpr ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (variant i32) (arg-forms Option<Sexpr>))llvm-value
    ;; The inverse of `compile-sexpr-field`, variant for variant. Each clause
    ;; compiles its own argument rather than one being hoisted out: `nil`(0)
    ;; has no argument at all, and a hoisted `compile-value` would emit that
    ;; argument's IR even for the variants that never use it.
    (case variant
      (0 (const-word builder 6))
      (1 (build-shl builder (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car arg-forms)) (const-word builder 3)))
      (2 (let ((args-ptr (alloca-args builder 1)))
           (store-arg builder args-ptr 0 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car arg-forms)))
           (build-call builder (get-function m "rt_f64_new") args-ptr 1)))
      (3 (build-or builder
                   (build-shl builder (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car arg-forms)) (const-word builder 3))
                   (const-word builder 4)))
      (4 (let ((b (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car arg-forms))))
           (build-or builder (build-shl builder (build-add builder b (const-word builder 1)) (const-word builder 3)) (const-word builder 6))))
      ;; `sym`(5)/`str`(6)/`bignum`(8)/`ratio`(9): the compiled argument is
      ;; already the fully tagged word this variant carries — a `Symbol`
      ;; immediate, a `Value::Str`, or `compile-bignum-literal`/
      ;; `compile-ratio-literal`'s `TAG_BOXED` result — so construction is
      ;; the identity, the same passthrough `compile-sexpr-field` reads back.
      ((5 6 8 9) (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car arg-forms)))
      ;; `cons`(7): the only variant with two fields, and so the only one
      ;; that has to root the first while the second is built.
      (7 (let* ((args-ptr (alloca-args builder 2))
                (car-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car arg-forms))))
           (store-arg builder args-ptr 0 car-v)
           (push-sexpr-root builder m car-v)
           (let ((cdr-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car (sexpr-cdr arg-forms)))))
             (store-arg builder args-ptr 1 cdr-v)
             (push-sexpr-root builder m cdr-v)
             (let ((result (build-call builder (get-function m "rt_cons") args-ptr 2)))
               (pop-sexpr-root builder m)
               (pop-sexpr-root builder m)
               result))))
      ;; `path`(10): the argument compiles to an already-tagged `Sexpr` list
      ;; of `sym`s (the shape `compile-sexpr-field`'s own `path` extraction
      ;; produces) — `rt_list_to_path` walks it at runtime and interns the
      ;; result, the construct-side mirror of `rt_path_to_list`.
      ;; `f32`(11): the same shape as `f64`(2) with the other box. The two
      ;; are separate variants precisely so this choice is made from the
      ;; declared type rather than guessed from a bit pattern that is
      ;; identical either way.
      (11 (let ((args-ptr (alloca-args builder 1)))
            (store-arg builder args-ptr 0 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car arg-forms)))
            (build-call builder (get-function m "rt_f32_new") args-ptr 1)))
      (10 (let ((args-ptr (alloca-args builder 1)))
            (store-arg builder args-ptr 0 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car arg-forms)))
            (build-call builder (get-function m "rt_list_to_path") args-ptr 1)))
      ;; `i8`(12)/`i16`(13)/`u8`(14)/`u16`(15)/`u32`(16): the compiled
      ;; argument is the plain normalized word, which says nothing about
      ;; which of the five types it is -- so the box is told, from the
      ;; variant, exactly the way `f32`/`f64` above are. `i32` needs no box
      ;; because the bare tagged word already means that width.
      ((12 13 14 15 16)
       (let ((args-ptr (alloca-args builder 2)))
         (store-arg builder args-ptr 0 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car arg-forms)))
         (store-arg builder args-ptr 1 (const-word builder (narrow-wsig variant)))
         (build-call builder (get-function m "rt_narrow_new") args-ptr 2)))
      (else (panic "compile-construct-sexpr: field type is not representable in compiled code yet"))))

;; `(field-get idx-unary-list kind-i64 obj-form)`
;; (Stage 6, extended by Stage 3 of the Sexpr/RtValue
;; unification plan — `docs/implementation-log.md` —
;; with `kind-i64`) — always a `BoxedObj::Struct`
;; `compile-construct-boxed-struct` built (this tag is
;; only ever synthesized by `Checker::check_defstruct`
;; as a field accessor's own body, never for a `Sexpr`
;; — see `core_bridge::translate_field`'s doc
;; comment, and every `defstruct` is `mutable`): the
;; object's own tagged value is handed straight to
;; `rt_struct_field_get` (which decodes/re-indexes it
;; itself, no `build-int-to-ptr` needed the way a
;; general-ADT box's raw pointer required), and the
;; raw `Sexpr` it returns is decoded back into this
;; field's own compiled representation by
;; `compile-sexpr-field` (reused verbatim — `kind`
;; *is* `Repr::field_kind`'s value, the
;; same `Sexpr`-variant numbering `compile-sexpr-field`
;; already expects; its own `idx` parameter is unused
;; for every kind this can produce, so `0` is passed).
;; `idx` (the *field* index, unrelated to `kind`) is
;; recovered from `idx-unary-list`'s own length
;; (`core_bridge::translate_field`'s doc comment
;; explains why it isn't simply a `Sexpr` `i32`) — no
;; header offset here, unlike the general-ADT box
;; layout's own variant-tag slot: a `BoxedObj::Struct`'s
;; own field vector has no variant-tag slot of its own.
;; The call stays a bare `build-call` even though
;; `rt_struct_field_get` can now raise: that index is a
;; compile-time constant the checker derived from the
;; `defstruct`'s own field list, so it is in range by
;; construction and the bound never fires. Only
;; `compile-vector-op`, whose index is a run-time
;; value, needs the protected form.
(defun compile-field-get ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((idx (sexpr-list-length (sexpr-car (sexpr-cdr e))))
           (kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
           (obj-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
           (obj-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup obj-form))
           (args-ptr (alloca-args builder 2)))
      (store-arg builder args-ptr 0 obj-v)
      (store-arg builder args-ptr 1 (const-word builder idx))
      (let ((raw (build-call builder (get-function m "rt_struct_field_get") args-ptr 2)))
        (compile-sexpr-field builder m raw kind 0))))

;; `(field-set idx-unary-list kind-i64 obj-form
;; value-form)` (Stage 6, extended the same way
;; `compile-field-get` was) — see that function's doc
;; comment; `compile-tag-struct-field` (the encode-side
;; mirror of `compile-sexpr-field`'s decode) tags the
;; already-compiled `value-form` before
;; `rt_struct_field_set` stores it. Evaluates to `Unit`
;; (`0`, `compile-unit`'s own convention), matching
;; `Expr::FieldSet`'s own checked type.
(defun compile-field-set ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((idx (sexpr-list-length (sexpr-car (sexpr-cdr e))))
           (kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
           (obj-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
           (value-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
           (obj-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup obj-form))
           (v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup value-form))
           (tagged-v (compile-tag-struct-field builder m v kind))
           (args-ptr (alloca-args builder 3)))
      (store-arg builder args-ptr 0 obj-v)
      (store-arg builder args-ptr 1 (const-word builder idx))
      (store-arg builder args-ptr 2 tagged-v)
      (let ((ignored (build-call builder (get-function m "rt_struct_field_set") args-ptr 3)))
        (const-word builder 0))))

;; `(vector-op method kind v-form arg-form...)` — a
;; `Vector<T>` builtin method
;; (`core_bridge::translate_vector_op`), lowered here rather than
;; through `compile-assoc` because these have no
;; compiled `defmethod` body. `kind` is `T`'s
;; `struct_field_kind`; the vector itself is a boxed
;; struct (tagged `Sexpr`), so its elements cross the
;; `BoxedObj::Struct` boundary tagged and need the same
;; `compile-tag-struct-field` (encode, `push`/`set`) /
;; `compile-sexpr-field` (decode, `get`/`pop`)
;; `compile-field-set`/`compile-field-get` use for a
;; fixed-arity field — the sole difference being a
;; *runtime* index (compiled from `idx-form`, already a
;; raw `i64`) rather than a compile-time-constant field
;; offset (`pop`, like `len`/`push`, has no index at
;; all — it always targets the last field). `len`/
;; `push`/`pop` are the three no-index primitives:
;; `rt_struct_field_count` (raw element count),
;; `rt_struct_push_field` (grow by one), and
;; `rt_struct_pop_field` (shrink by one, returning the
;; removed element — the caller checks the count
;; first, so the empty case never reaches it). An
;; out-of-range `get`/`set` index raises the
;; interpreter's own `Vector: index N out of bounds`
;; (`typelisp_rt::checked_field_index`), so those two
;; calls go through `emit-direct-call`: raising means
;; unwinding, and an unwind is caught only by the
;; frame that made the call, so a bare `build-call`
;; inside a `catch`/`unwind-protect` region would skip
;; that region's cleanups. `pop`/`len`/`push` cannot
;; raise and stay bare calls.
;; No extra GC-rooting beyond what `compile-field-set`
;; already relies on: the receiver flows in as an
;; ordinary (env-rooted) value, and `rt_struct_push_
;; field`/`rt_struct_pop_field` allocate nothing.
(defun compile-vector-op ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((method (sexpr-str (sexpr-car (sexpr-cdr e))))
           (kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr e))))))
      (if (equal method "new")
          (compile-construct-boxed-struct m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))) (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
      (let* ((v-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
             (v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup v-form)))
        (case method
          ("len"
           (let ((args-ptr (alloca-args builder 1)))
             (store-arg builder args-ptr 0 v)
             (build-call builder (get-function m "rt_struct_field_count") args-ptr 1)))
          ("pop"
           ;; `pop` returns `Option<T>`: an empty
           ;; vector is `None`, not a bounds
           ;; panic (unlike `get`/`set`) — so
           ;; this needs real control flow,
           ;; following `compile-hashtable-op`'s
           ;; `get`/`remove` "alloca a merge
           ;; slot, branch, store each arm's
           ;; result, load after the merge
           ;; block" shape. `rt_struct_field_
           ;; count` is checked first (`= 0` -> a
           ;; compile-time-known `None`), then
           ;; only the confirmed-nonempty branch
           ;; calls `rt_struct_pop_field` — safe
           ;; with no race, single-threaded
           ;; compiled code can't shrink the
           ;; vector between the two calls. The
           ;; popped value is already a properly
           ;; tagged `Sexpr` (`push`'s own
           ;; `compile-tag-struct-field` call
           ;; tagged it going in), so it flows
           ;; into `Some` via `rt_data_new`
           ;; unchanged — no `compile-sexpr-
           ;; field` decode step, exactly as
           ;; `compile-hashtable-op`'s `get`/
           ;; `remove` pass their own raw looked-
           ;; up value straight through.
           ;; `option-name-form` is the node's
           ;; one trailing operand
           ;; (`translate_vector_method`'s
           ;; `pop`-only addition), in the same
           ;; slot `get`/`set`'s `idx-form`
           ;; would occupy.
           (let* ((option-name-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                  (option-name-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup option-name-form))
                  (args-ptr (alloca-args builder 1)))
             (store-arg builder args-ptr 0 v)
             (let* ((count (build-call builder (get-function m "rt_struct_field_count") args-ptr 1))
                    (empty (build-icmp-eq builder count (const-word builder 0)))
                    (then-block (append-block cur-fn "vec-pop-empty"))
                    (else-block (append-block cur-fn "vec-pop-nonempty"))
                    (merge-block (append-block cur-fn "vec-pop-merge"))
                    (slot (alloca-args builder 1)))
               (build-cond-br builder empty then-block else-block)
               (position-at-end builder then-block)
               (let ((none-args (alloca-args builder 2)))
                 (store-arg builder none-args 0 option-name-v)
                 (store-arg builder none-args 1 (const-word builder 1))
                 (let ((none-box (build-call builder (get-function m "rt_data_new") none-args 2)))
                   (push-permanent-sexpr-root builder m none-box)
                   (let ((ignored (store-arg builder slot 0 none-box)))
                     (build-br builder merge-block))))
               (position-at-end builder else-block)
               (let* ((raw (build-call builder (get-function m "rt_struct_pop_field") args-ptr 1))
                      (some-args (alloca-args builder 3)))
                 (store-arg builder some-args 0 option-name-v)
                 (store-arg builder some-args 1 (const-word builder 0))
                 (store-arg builder some-args 2 raw)
                 (let ((some-box (build-call builder (get-function m "rt_data_new") some-args 3)))
                   (push-permanent-sexpr-root builder m some-box)
                   (let ((ignored (store-arg builder slot 0 some-box)))
                     (build-br builder merge-block))))
               (position-at-end builder merge-block)
               (load-raw builder slot 0))))
          ("push"
           (let* ((x-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                  (x (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup x-form))
                  (tagged-x (compile-tag-struct-field builder m x kind))
                  (args-ptr (alloca-args builder 2)))
             (store-arg builder args-ptr 0 v)
             (store-arg builder args-ptr 1 tagged-x)
             (let ((ignored (build-call builder (get-function m "rt_struct_push_field") args-ptr 2)))
               (const-word builder 0))))
          (else (let* ((idx-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                       (idx (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup idx-form)))
                  (if (equal method "get")
                       (let ((args-ptr (alloca-args builder 2)))
                         (store-arg builder args-ptr 0 v)
                         (store-arg builder args-ptr 1 idx)
                         (let ((raw (emit-direct-call builder m cur-fn (get-function m "rt_struct_field_get") args-ptr 2 protect)))
                           (compile-sexpr-field builder m raw kind 0)))
                       (let* ((x-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))
                              (x (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup x-form))
                              (tagged-x (compile-tag-struct-field builder m x kind))
                              (args-ptr (alloca-args builder 3)))
                         (store-arg builder args-ptr 0 v)
                         (store-arg builder args-ptr 1 idx)
                         (store-arg builder args-ptr 2 tagged-x)
                         (let ((ignored (emit-direct-call builder m cur-fn (get-function m "rt_struct_field_set") args-ptr 3 protect)))
                           (const-word builder 0)))))))))))

;; `(hashtable-op method key-kind val-kind ht-form
;; ...)` — a `HashTable<K,V>` builtin
;; (`core_bridge::translate_hashtable_op`), lowered to the
;; `rt_hashtable_*` family. `new` builds an empty map (no
;; operands); `count` returns a raw `i64`; `clear`
;; returns `Unit`; and `keys`/`values`/`entries` each
;; build a fresh `Vector` (`entries`'s of `cons-cell`s)
;; that — like a `compile-construct-boxed-struct` result
;; — is `push-permanent-sexpr-root`ed on the spot so a
;; later allocation can't reclaim it.
;;
;; `get`/`set`/`remove` are **not** here, and no longer
;; anywhere in this file: they are prelude methods
;; written on the `bucket-*` primitives, because hashing
;; a key and comparing two keys are the key type's own
;; `sxhash`/`equals`. What is left here is what this
;; layer can answer alone — a bucket by hash, an entry by
;; index — with the key and value tagged/decoded by
;; `key-kind`/`val-kind` exactly as a struct field is.
(defun compile-hashtable-op ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let ((method (sexpr-str (sexpr-car (sexpr-cdr e)))))
      (if (equal method "new")
          ;; The table's own runtime identity travels in the name slot: a type
          ;; key includes its instantiation now, and an empty table has no
          ;; field to read one off. `rt_hashtable_new` interns what it is
          ;; given.
          (let* ((name-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                 (name-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup name-form))
                 (args-ptr (alloca-args builder 1)))
            (store-arg builder args-ptr 0 name-v)
            (let ((result (build-call builder (get-function m "rt_hashtable_new") args-ptr 1)))
              (push-permanent-sexpr-root builder m result)
              result))
          (let* ((key-kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
                 (val-kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                 (option-type-name-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))
                 (ht-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))
                 (ht (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup ht-form)))
            (case method
            ;; The bucket family. Every operand but the key and the value is
            ;; a raw word already (the hash and the index are `i32`s), so
            ;; only those two are tagged/decoded — by `key-kind`/`val-kind`,
            ;; exactly as a struct field is.
            ("bucket-count"
             (let* ((h (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))))
                    (args-ptr (alloca-args builder 2)))
               (store-arg builder args-ptr 0 ht)
               (store-arg builder args-ptr 1 h)
               (build-call builder (get-function m "rt_hashtable_bucket_count") args-ptr 2)))
            (("bucket-key" "bucket-value")
             (let* ((h (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))))
                    (i (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))))))
                    (args-ptr (alloca-args builder 3)))
               (store-arg builder args-ptr 0 ht)
               (store-arg builder args-ptr 1 h)
               (store-arg builder args-ptr 2 i)
               (let ((raw (build-call builder (get-function m (if (equal method "bucket-key") "rt_hashtable_bucket_key" "rt_hashtable_bucket_value")) args-ptr 3)))
                 (compile-sexpr-field builder m raw (if (equal method "bucket-key") key-kind val-kind) 0))))
            ("bucket-put"
             (let* ((h (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))))
                    (i (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))))))
                    (k (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))))))
                    (v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))))))))
                    (tk (compile-tag-struct-field builder m k key-kind))
                    (tv (compile-tag-struct-field builder m v val-kind))
                    (args-ptr (alloca-args builder 5)))
               (store-arg builder args-ptr 0 ht)
               (store-arg builder args-ptr 1 h)
               (store-arg builder args-ptr 2 i)
               (store-arg builder args-ptr 3 tk)
               (store-arg builder args-ptr 4 tv)
               (let ((ignored (build-call builder (get-function m "rt_hashtable_bucket_put") args-ptr 5)))
                 (const-word builder 0))))
            ("bucket-delete"
             (let* ((h (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))))))
                    (i (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr (sexpr-cdr e))))))))))
                    (args-ptr (alloca-args builder 3)))
               (store-arg builder args-ptr 0 ht)
               (store-arg builder args-ptr 1 h)
               (store-arg builder args-ptr 2 i)
               (let ((ignored (build-call builder (get-function m "rt_hashtable_bucket_delete") args-ptr 3)))
                 (const-word builder 0))))
            ("count"
             (let ((args-ptr (alloca-args builder 1)))
               (store-arg builder args-ptr 0 ht)
               (build-call builder (get-function m "rt_hashtable_count") args-ptr 1)))
            ("clear"
             (let ((args-ptr (alloca-args builder 1)))
               (store-arg builder args-ptr 0 ht)
               (let ((ignored (build-call builder (get-function m "rt_hashtable_clear") args-ptr 1)))
                 (const-word builder 0))))
            ;; `keys`/`values`/`entries`, each of which builds a `Vector<_>`
            ;; whose instantiation only the checker knows — so the name slot's
            ;; key goes with the table as a second argument.
            (else (let* ((name-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup option-type-name-form))
                         (args-ptr (alloca-args builder 2)))
                     (store-arg builder args-ptr 0 ht)
                     (store-arg builder args-ptr 1 name-v)
                     (let ((result (build-call builder (get-function m (append "rt_hashtable_" method)) args-ptr 2)))
                       (push-permanent-sexpr-root builder m result)
                       result))))))))

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
;; (`Repr::field_kind`'s numbering — the
;; same one `compile-field-get` already uses) untags
;; it back to the global's own declared representation
;; via `compile-sexpr-field` (this function's own
;; decode step, shared with `compile-field-get`). An
;; enum-typed global (`kind = 6`, since the enum-
;; representation unification retired the old
;; shift-tagged `kind = 10` special case) is just
;; another passthrough value here, no different from
;; a boxed struct or `Str` global.
(defun compile-global ((m llvm-module) (fn-name string) (builder llvm-builder) (e Option<Sexpr>))llvm-value
    (let* ((id (sexpr-i32 (sexpr-car (sexpr-cdr e))))
           (kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
           (args-ptr (alloca-args builder 1)))
      (store-arg builder args-ptr 0 (const-word builder id))
      (let ((raw (build-call builder (get-function m "rt_global_get") args-ptr 1)))
        (compile-sexpr-field builder m raw kind 0))))

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
(defun compile-set-global ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((id (sexpr-i32 (sexpr-car (sexpr-cdr e))))
           (kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
           (value-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
           (v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup value-form))
           (tagged-v (compile-tag-struct-field builder m v kind))
           (args-ptr (alloca-args builder 2)))
      (store-arg builder args-ptr 0 (const-word builder id))
      (store-arg builder args-ptr 1 tagged-v)
      (let ((ignored (build-call builder (get-function m "rt_global_set") args-ptr 2)))
        v)))

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
(defun compile-global-init ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((kind (sexpr-i32 (sexpr-car (sexpr-cdr e))))
           (value-form (sexpr-car (sexpr-cdr (sexpr-cdr e))))
           (v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup value-form))
           (tagged-v (compile-tag-struct-field builder m v kind))
           (args-ptr (alloca-args builder 1)))
      (store-arg builder args-ptr 0 tagged-v)
      (build-call builder (get-function m "rt_global_new") args-ptr 1)))

;; `(catch tag-form kind body)` — run `body`, and if a `(throw 'tag v)` fired
;; anywhere it reached, produce `v` here instead.
;;
;; Three blocks and a slot, the same shape `compile-loop` already uses for
;; `break`: the body runs with this region's **dispatch block** installed as
;; `protect`, so every call it emits comes back through `check-unwind`; the
;; dispatch block decides whether the unwind in flight is this catch's; and
;; `merge` is where both paths meet with the result in the slot.
;;
;; `root-base` is read before the body and the dispatch block truncates back
;; to it, exactly as `compile-break` does off `loop-root-base`: an unwind
;; skipped every `pop-sexpr-root` between the throw and here, and the
;; trampoline only undid the callee's own.
;;
;; The thrown value arrives from the runtime as a tagged `Sexpr`, so it is
;; decoded back to this catch's own representation (`compile-sexpr-field`,
;; the same decoder a struct field read uses) — `kind` is what
;; `core_bridge` baked in for exactly this.
(defun compile-catch ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((tag-form (sexpr-car (sexpr-cdr e)))
           (kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
           (body (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
           (tag (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup tag-form))
           (slot (alloca-args builder 1))
           (root-base (build-call builder (get-function m "rt_root_count") (alloca-args builder 0) 0))
           (pad (append-block cur-fn "catch-pad"))
           (claim (append-block cur-fn "catch-claim"))
           (onward (append-block cur-fn "catch-onward"))
           (merge (append-block cur-fn "catch-merge"))
           (v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (Option::some pad) exit-cleanup body)))
      (if (block-terminated? builder)
          ()
          (let ((ignored (store-arg builder slot 0 v)))
            (build-br builder merge)))
      (position-at-end builder pad)
      (let ((truncate-args (alloca-args builder 1)))
        (store-arg builder truncate-args 0 root-base)
        (let ((ignored2 (build-call builder (get-function m "rt_truncate_sexpr_roots") truncate-args 1))) ()))
      (let ((tag-args (alloca-args builder 1)))
        (store-arg builder tag-args 0 tag)
        (let ((mine (build-call builder (get-function m "rt_throw_matches") tag-args 1)))
          (build-cond-br builder mine claim onward)))
      (position-at-end builder claim)
      (let ((thrown (build-call builder (get-function m "rt_throw_take_value") (alloca-args builder 0) 0)))
        ;; `kind = 0` means the checker never pinned a type
        ;; on this tag — nothing anywhere throws it, so this
        ;; block is unreachable and there is nothing to
        ;; decode. `(catch 'unused ...)` and a `catch` whose
        ;; body only `break`s both land here. Storing the
        ;; raw word keeps the block well-formed;
        ;; `compile-sexpr-field` would panic on it.
        (if (eq kind 0)
            (store-arg builder slot 0 thrown)
            (store-arg builder slot 0 (compile-sexpr-field builder m thrown kind 0)))
        (build-br builder merge))
      (position-at-end builder onward)
      (emit-unwind-onward builder m protect)
      (position-at-end builder merge)
      (load-raw builder slot 0)))

;; `(throw tag-form kind value-form)` — leave for the nearest dynamically
;; enclosing `catch` on this tag.
;;
;; The value is encoded to a tagged `Sexpr` first (`compile-tag-struct-field`,
;; `compile-sexpr-field`'s mirror): it is about to be held by the runtime,
;; where a raw machine word carries no way to tell an integer from a heap
;; reference — and the collector has to be able to trace it while the unwind
;; travels (`Heap::set_in_flight_throw`).
;;
;; The call goes through `emit-rt-call` like any other: a `throw` written
;; inside the very region that catches it is the one case where the unwind
;; never leaves this function, and it has to be caught all the same.
(defun compile-throw ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((tag-form (sexpr-car (sexpr-cdr e)))
           (kind (sexpr-i32 (sexpr-car (sexpr-cdr (sexpr-cdr e)))))
           (value-form (sexpr-car (sexpr-cdr (sexpr-cdr (sexpr-cdr e)))))
           (tag (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup tag-form))
           (v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup value-form))
           (tagged (if (eq kind 0) v (compile-tag-struct-field builder m v kind)))
           (args-ptr (alloca-args builder 2)))
      (store-arg builder args-ptr 0 tag)
      (store-arg builder args-ptr 1 tagged)
      (emit-rt-call builder m cur-fn "rt_throw" "rt_protected_throw" args-ptr 2 protect)))

;; `(unwind-protect protected cleanup)` — run `protected`, then `cleanup`,
;; whichever way `protected` left.
;;
;; The cleanup is emitted **twice**, once per kind of exit: inline on the
;; ordinary path, and again in the dispatch block for the unwinding one. Two
;; copies rather than one shared block with a continuation flag, because the
;; two paths differ in where they go afterwards and in nothing else — and a
;; duplicated form is cheaper to read than a dispatch table.
;;
;; Both copies are compiled with the **enclosing** `protect`, not this
;; region's: a cleanup is not protected by its own `unwind-protect` (CLHS), so
;; a throw raised inside one belongs to whatever encloses it — and replaces
;; the exit already in flight, which is what the runtime's single parked slot
;; does by construction.
;;
;; A `break`/`return` out of `protected` still skips the cleanup: it branches
;; straight to the loop's exit block, past both copies. That is the one gap
;; left here, recorded in `docs/dev/TODO.md`; closing it means giving the
;; cleanup a third copy at the `break` site, which is a change to
;; `compile-break` rather than to anything on the unwinding path.
(defun compile-unwind-protect ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((protected-form (sexpr-car (sexpr-cdr e)))
           (cleanup-form (sexpr-car (sexpr-cdr (sexpr-cdr e))))
           (slot (alloca-args builder 1))
           (root-base (build-call builder (get-function m "rt_root_count") (alloca-args builder 0) 0))
           (pad (append-block cur-fn "cleanup-pad"))
           (merge (append-block cur-fn "cleanup-merge"))
           (xexit (match loop-exit
                               ((Some eb) (Option::some (append-block cur-fn "cleanup-exit")))
                               (None (Option::none))))
           (v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base (Option::some pad) xexit protected-form)))
      (if (block-terminated? builder)
        ()
        (let* ((ignored (store-arg builder slot 0 v))
               (ignored2 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup cleanup-form)))
          (if (block-terminated? builder)
              ()
              (build-br builder merge))))
    (position-at-end builder pad)
    (let ((truncate-args (alloca-args builder 1)))
      (store-arg builder truncate-args 0 root-base)
      (let ((ignored3 (build-call builder (get-function m "rt_truncate_sexpr_roots") truncate-args 1))) ()))
    (let ((ignored4 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup cleanup-form)))
      (if (block-terminated? builder)
          ()
          (emit-unwind-onward builder m protect)))
    (match xexit
      ((Some xe)
       (let ((ignored5 (position-at-end builder xe)))
         (let ((xargs (alloca-args builder 1)))
           (store-arg builder xargs 0 root-base)
           (let ((ignored6 (build-call builder (get-function m "rt_truncate_sexpr_roots") xargs 1))) ()))
         (let ((ignored7 (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup cleanup-form)))
           (if (block-terminated? builder)
               ()
               (emit-static-exit-onward builder m loop-exit loop-root-base exit-cleanup)))))
      (None ()))
    (position-at-end builder merge)
    (load-raw builder slot 0)))

;; `(panic msg-form)` — `Expr::Panic`, always
;; `Str`-typed (`Checker::check_panic` requires it), so
;; no `kind`/scrutinee-shape dispatch is needed the way
;; `Match`/`FieldGet` need — `msg-form` compiles down to
;; the exact tagged `Sexpr::Str` representation
;; `compile-str` already produces, handed straight to
;; `rt_panic` — which *unwinds* with the message rather
;; than returning, so the boundary that entered
;; compiled code turns it back into the same catchable
;; `EvalError::Panic` an interpreted `(panic ...)`
;; produces. Nothing changes on this side for that: the
;; unwinding is all in `rt_panic`'s own
;; `extern "C-unwind"` declaration, and a plain `call`
;; to a function that unwinds needs no landing pad in
;; this frame — only a frame the unwinder can walk,
;; which MCJIT and the AOT linker both give us (see
;; `tests/compiled_unwind_test.rs`). `rt_match_fail`
;; still aborts, but that is a checker-guaranteed
;; unreachable, not a user-visible failure.
;; `Expr::Panic`'s own checked type is `Never`, so
;; nothing downstream ever reads this call's return
;; value for real.
(defun compile-panic ((m llvm-module) (fn-name string) (builder llvm-builder) (env Scope<llvm-value>) (fn-env Scope<llvm-function>) (captured Option<Sexpr>) (cur-fn llvm-function) (loop-exit Option<llvm-basic-block>) (loop-slot Option<llvm-value>) (loop-root-base Option<llvm-value>) (protect Option<llvm-basic-block>) (exit-cleanup Option<llvm-basic-block>) (e Option<Sexpr>))llvm-value
    (let* ((msg-form (sexpr-car (sexpr-cdr e)))
           (msg-v (compile-value m fn-name builder env fn-env captured cur-fn loop-exit loop-slot loop-root-base protect exit-cleanup msg-form))
           (args-ptr (alloca-args builder 1)))
      (store-arg builder args-ptr 0 msg-v)
      (emit-rt-call builder m cur-fn "rt_panic" "rt_protected_panic" args-ptr 1 protect)))

(defun compile-function ((m llvm-module) (name string) (param-names Option<Sexpr>) (body Option<Sexpr>)) llvm-module
    (let* ((f (add-function m name))
           (b (append-block f "entry"))
           (builder (llvm-builder::create)))
      (position-at-end builder b)
      (let ((env (new-env)))
        (bind-params env builder m f param-names 0)
        (retain-bindings builder m env param-names)
        (let* ((fn-env (new-fn-env))
               (v (compile-value m name builder env fn-env '() f (Option::none) (Option::none) (Option::none) (Option::none) (Option::none) body)))
          (release-bindings builder m env param-names)
(build-ret builder v)
m))))

"#;

// (Removed the interpreted island loader `load` in interp-closure removal
// Stage 8c.) It read/checked/`exec`d `SOURCE` without installing the native
// bitcode, so calling `compile-function` afterward tree-walked the island —
// which built interpreted closures for its own `labels`/`lambda`s. With those
// closures gone, that path can no longer run; [`load_aot`] (the only loader
// now, behind `crate::load_compiler`) installs the native bodies so the island
// runs compiled.

/// The precompiled compiler-island bitcode (interp-closure removal Stage 3),
/// embedded so [`load_aot`] needs no filesystem access at runtime. Kept in
/// sync with `SOURCE` by `scripts/regen-compiler-island.sh` and the
/// `island_artifacts_are_fresh` test.
/// The committed island dump: the checked state the island's 121 `defun`s and
/// one `defmacro` produce, paired with the bitcode holding their native
/// bodies. Written by `scripts/regen-compiler-island.sh`, applied by
/// [`load_aot`].
pub const ISLAND_DUMP: &[u8] = include_bytes!("compiler_island.typld");

/// What to run when the committed dump no longer matches [`SOURCE`].
pub const REGEN_SCRIPT: &str = "scripts/regen-compiler-island.sh";

/// Loads the compiler island as **native code** (interp-closure removal
/// Stage 4): the committed dump's checked state and definitions — which
/// registers each island `defun`'s `FnDef` and checker entry but allocates no
/// closures (a `defun` only *builds* a closure when its body is *called*
/// interpreted, which never happens after this) — followed by installing the
/// same dump's native function bodies into [`Interp`]'s compiled-function
/// table.
///
/// Reading and type-checking `SOURCE` here is what the dump replaced: 57ms of
/// read, 307ms of check and a further 70ms of re-reading it to hash, out of a
/// 1.50s `typl` startup (`typl-bench-prelude`).
///
/// After this, calling `compile-function` (directly via `(compile ...)`, or
/// transitively when a user closure is JIT-compiled at definition time)
/// dispatches to the native island rather than tree-walking it — so the
/// island's own `labels`/`lambda` bodies never become interpreted closures,
/// which is what lets interpreted closures be removed entirely (Stage 8).
/// `SOURCE` being fixed and the dump a committed, digest-checked artifact, any
/// failure here is a build/bug condition, not a user error — hence the panics.
pub fn load_aot(heap: &mut Heap, chk: &mut Checker, interp: &mut Interp) {
    // The island's whole purpose is emitting IR, so this is where the backend
    // that does the emitting becomes available: every path that can reach an
    // `llvm-*` builtin — the JIT driver, `compile-file`, the two
    // bootstrappers, a test evaluating one directly — comes through here
    // first. An `Interp` built *without* loading the island has no business
    // reaching those builtins, and gets a clear invariant error rather than a
    // silently missing backend (see `eval::interp::set_llvm_builtin_hook`).
    crate::compile::install_llvm_backend();

    let units = typelisp_front::dump::parse(ISLAND_DUMP, "compiler island")
        .unwrap_or_else(|e| panic!("compiler: {}", e));
    let unit = units.first().unwrap_or_else(|| panic!("compiler: the committed dump holds no units"));
    let state = crate::compile::dump::read_types(unit, "compiler island")
        .unwrap_or_else(|e| panic!("compiler: {}", e));
    typelisp_front::dump::verify_digest(&state, SOURCE, REGEN_SCRIPT).unwrap_or_else(|e| panic!("{}", e));
    crate::compile::dump::load_unit(heap, chk, interp, state, unit.bitcode)
        .unwrap_or_else(|e| panic!("compiler: {}", e));
    interp.push_dump_source(std::borrow::Cow::Borrowed(ISLAND_DUMP));
}
