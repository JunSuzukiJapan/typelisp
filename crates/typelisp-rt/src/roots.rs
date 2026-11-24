//! GC root bookkeeping for compiled code.

use typelisp_abi::{active_heap, decode, encode, fatal};

// ---- Stage 4: GC root safety -------------------------------------------

/// Registers a `Sexpr`-typed value as a GC root for as long as it's live in
/// a compiled frame — closing the gap [`rt_cons`]'s doc comment calls out:
/// without this, a GC that `rt_cons` (or any other allocating `rt_*` call)
/// triggers internally could reclaim a cons cell some *other* live value in
/// the calling frame still points to, since nothing makes that value
/// visible to `Heap::gc`'s root walk otherwise. `compiler.rs`'s future
/// rooting pass (paralleling its existing `ClosureBox` retain/release
/// insertion) is expected to wrap every `Sexpr`-typed local's lexical scope
/// in a push here / [`rt_pop_sexpr_root`] there, the same way retain/release
/// already wrap a `Fn`-typed one's.
///
/// Returns its argument unchanged (like `build-closure-retain`), so a
/// caller can chain it directly around the value it's rooting rather than
/// needing a separate statement.
///
/// # Safety
///
/// The words must be valid; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_push_sexpr_root(tagged: i64) -> i64 {
    let tagged = tagged;
    active_heap().push_root(decode(tagged));
    tagged
}

/// The inverse of [`rt_push_sexpr_root`] — pops the most recently pushed
/// root and returns it (decoded back to its tagged form), for the matching
/// end of whatever lexical scope pushed it. Fatal if nothing is on the root
/// stack to pop: every call site is expected to be paired 1:1 with an
/// earlier [`rt_push_sexpr_root`], so an empty stack here is a
/// `compiler.rs`-side bug, not a recoverable condition.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_pop_sexpr_root() -> i64 {
    match active_heap().pop_root() {
        Some(v) => encode(v),
        None => fatal("rt_pop_sexpr_root: root stack was empty"),
    }
}

/// Registers a `Sexpr`-typed value as a *permanent* GC root — for a value
/// stored into a general-ADT box's field (`compiler.rs`'s
/// `compile-construct-box-fields`), not a call-stack-scoped local.
/// [`rt_push_sexpr_root`]'s root lives on `Heap`'s ordinary `roots` stack,
/// which every caller above and below relies on strict LIFO pairing for
/// (push on scope entry, pop on scope exit) — a box field has no such scope:
/// it must stay reachable for as long as the box itself does, which can
/// outlive the activation that built it. Pushing it there with no matching
/// pop would desync every *other* `rt_pop_sexpr_root` call still to come in
/// the same thread (the next one would pop this field's root instead of
/// whatever it actually owns). [`typelisp_mem::Heap::push_permanent_root`]
/// exists precisely to take such a value off to one side, in a `Vec`
/// `Heap::gc`'s mark phase walks but no `rt_*` function ever pops from — see
/// that method's doc comment. No `rt_pop_permanent_sexpr_root` exists, by
/// design: this leaks one root slot per call, forever, the same trade-off
/// the box itself already makes (never `build-free`'d — `compiler.rs`'s
/// `compile-construct-box` doc comment).
///
/// Returns its argument unchanged, like [`rt_push_sexpr_root`].
///
/// # Safety
///
/// Same as [`rt_push_sexpr_root`].
#[no_mangle]
pub unsafe extern "C" fn rt_push_permanent_sexpr_root(tagged: i64) -> i64 {
    let tagged = tagged;
    active_heap().push_permanent_root(decode(tagged));
    tagged
}

/// Returns the active `Heap`'s current root count (`Heap::root_count`), as a
/// raw host index — never a tagged `Sexpr`, unlike every other `rt_*`
/// function's `i64` payload. `compiler.rs`'s `retain-bindings`/
/// `bind-let-values` call this *immediately before* their own
/// [`rt_push_sexpr_root`] call for a `kind = 2` (`Sexpr`-typed) binding, to
/// capture the exact stack position that root is about to occupy (`push_root`
/// appends at the end, so "current count" *is* "the new root's index"). That
/// index is then stashed in a second word of the binding's own slot
/// (`compiler.rs`'s `bind-params`/`bind-captures`/`bind-let-values` all now
/// allocate two words per binding rather than one) so a later `setf` to the
/// same binding (`compile-set`) can hand it straight to
/// [`rt_set_sexpr_root`] — see that function's doc comment for why an
/// update-in-place, not a second push, is what a reassignment actually
/// needs.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_root_count() -> i64 {
    active_heap().root_count() as i64
}

/// `setf`'s GC-root counterpart to [`rt_push_sexpr_root`]: overwrites the
/// root at stack position `idx` (a raw host index, *not* a tagged
/// `Sexpr` — see [`rt_root_count`]) with `tagged` (`Heap::set_root`).
///
/// A `let`/parameter/captured binding's GC root is pushed exactly once, at
/// bind time, holding whatever value the binding started with
/// (`rt_push_sexpr_root`, called from `compiler.rs`'s `retain-bindings`/
/// `bind-let-values`). `compile-set` only ever overwrites the binding's own
/// value *slot* in place — it never touches `Heap.roots` — so without this,
/// a `setf` that reassigns a `kind = 2` binding to a freshly built value
/// leaves that new value completely unrooted for the rest of the binding's
/// scope: the existing root stays pinned to the *original* value (itself
/// now harmlessly over-retained, not a correctness problem), while the new
/// one is exposed to the very next GC any unrelated allocation triggers.
/// `crates/typelisp-rt/src/lib.rs`'s own
/// `a_setf_reassigned_sexpr_value_is_corrupted_by_a_gc_triggered_by_other_allocations_without_rt_set_sexpr_root`
/// test demonstrates that corruption directly, the same way
/// [`rt_push_sexpr_root`]'s doc comment points at
/// `an_unrooted_value_is_corrupted_by_a_gc_triggered_by_other_allocations`
/// for the never-rooted-at-all case this complements. A binding keeps the
/// same root-stack slot for its whole lifetime (`rt_root_count`, called
/// once when the root is first pushed, hands back that fixed index), so
/// every subsequent `setf` to the same binding just updates that one slot
/// again — `O(1)` regardless of how many times it's reassigned (e.g. inside
/// a loop), unlike pushing a fresh root per `setf` would be (which has no
/// matching pop count known until runtime).
///
/// Returns `tagged` unchanged, like [`rt_push_sexpr_root`].
///
/// # Safety
///
/// The words must be valid; a
/// `Heap` must already be registered on this thread; `idx` must be a
/// valid index into that `Heap`'s current root stack (always true in
/// practice — it's always a value `rt_root_count` itself returned earlier in
/// the same dynamic scope, never user input).
#[no_mangle]
pub unsafe extern "C" fn rt_set_sexpr_root(idx: i64, tagged: i64) -> i64 {
    let idx = idx as usize;
    let tagged = tagged;
    active_heap().set_root(idx, decode(tagged));
    tagged
}

/// `compiler.rs`'s `compile-break`/`compile-return` call this, with
/// `len` set to the root count `compile-loop` read (`rt_root_count`)
/// right before entering the loop's own body, immediately before building
/// the jump to the loop's exit block — discarding, in one call
/// (`Heap::truncate_roots`), every `Sexpr`-typed GC root any number of
/// nested `let`/`match` scopes between the `break`/`return` site and the
/// loop pushed and never got the chance to pop, since a `break`/`return`
/// jumps straight past their own ordinary pop-on-scope-exit code
/// (`compile-let`'s `unroot-let-sexpr-values`, `compile-match`'s
/// `pop-sexpr-root` at its merge block — both skip that pop specifically
/// *because* the block is already terminated by the jump this truncates
/// for). Unlike [`rt_pop_sexpr_root`], which is fatal on an empty stack,
/// this is a no-op if `len` >= rt_root_count()` already (a `break`/
/// `return` with no scopes open above the loop itself) — the same
/// "truncating to at-or-past the current length does nothing" behavior
/// [`typelisp_mem::Heap::truncate_roots`] gets from `Vec::truncate`.
///
/// Returns `0` — nothing meaningful to hand back, like [`rt_heap_init`].
///
/// # Safety
///
/// The words must be valid; a
/// `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_truncate_sexpr_roots(len: i64) -> i64 {
    let len = len as usize;
    active_heap().truncate_roots(len);
    0
}
