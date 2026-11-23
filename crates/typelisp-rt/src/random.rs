//! `random-state`.

use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, decode, encode, fatal, raise};
use typelisp_mem::{BoxId, Value};

// ---- random-state -------------------------------------------------------
//
// A `random-state` is an ordinary boxed heap object holding one `u64` seed,
// so nothing about it lives outside the heap and all three of its builtins
// lower to shims here.

/// One step of a 64-bit xorshift generator (period `2^64 - 1` over the
/// nonzero states — these exact shift/xor constants are a full-cycle
/// permutation of them, so a nonzero seed can never reach `0`).
///
/// Public, and the interpreter's `random-state-next` calls it rather than
/// keeping its own copy: a compiled draw and an interpreted draw from the
/// same seed must produce the same number, which two implementations of the
/// same three lines cannot promise.
pub fn xorshift64_step(x: u64) -> u64 {
    let mut x = x;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// A fresh entropy seed for `make-random-state-fresh` — `| 1` guarantees
/// non-zero (the one fixed point [`xorshift64_step`] can't escape).
///
/// Public for the same reason [`xorshift64_step`] is: the interpreter's
/// `eval_make_random_state_fresh` calls it rather than keeping a second copy.
pub fn fresh_random_seed() -> u64 {
    typelisp_mem::host::unix_time()
        // A clock set before 1970 is as good a source of entropy as any.
        .map_or_else(|before| before.as_nanos() as u64, |d| d.as_nanos() as u64)
        | 1
}

/// The state a user-supplied seed names, for `seed-random-state`.
///
/// Not the seed itself. [`xorshift64_step`] has exactly one fixed point, 0,
/// and a stream started there stays there forever, so the state space is
/// `1..=u64::MAX` and *some* map from the user's integer into it is
/// unavoidable. The obvious one, `seed as u64 | 1`, is wrong: it sends every
/// even seed onto its odd neighbour, so `(seed-random-state 0)` and
/// `(seed-random-state 1)` would name the same stream and half of all seeds
/// would be aliases of the other half. A caller reaching for a seed wants
/// reproducibility, and silently sharing a stream with a different seed is
/// the one property that breaks it.
///
/// So: SplitMix64's finalizer, which is a *bijection* on `u64`. Exactly one
/// input maps to 0 and is redirected to a constant; every other pair of
/// distinct seeds gets a distinct state. One collision is the least any map
/// into a space one element smaller than its domain can have.
///
/// The seed arrives as `i64` (the language hands over an `i32`, sign
/// extended) and is reinterpreted, not clamped, so `-1` and `1` are
/// different seeds.
pub fn seeded_random_state(seed: i64) -> u64 {
    let mut z = (seed as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    if z == 0 {
        0x9E37_79B9_7F4A_7C15
    } else {
        z
    }
}

/// `make-random-state-fresh` for compiled code — a brand new, independently
/// seeded stream (`eval_make_random_state_fresh`). Nullary; the result is a
/// fresh allocation, as unrooted as [`rt_str_new`]'s until a caller protects
/// it.
///
/// # Safety
///
/// A `Heap` must already be registered on this thread. Takes no arguments,
/// so `args`/`argc` are unread.
#[no_mangle]
pub unsafe extern "C" fn rt_make_random_state_fresh(_args: *const i64, _argc: u32) -> i64 {
    encode(active_heap().alloc_random_state(fresh_random_seed()))
}

/// `seed-random-state` for compiled code — the stream a given seed names
/// (`eval_seed_random_state`), through [`seeded_random_state`]. Like
/// [`rt_make_random_state_fresh`] the result is a fresh, still-unrooted
/// allocation.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args[0]` must decode to an integer; a `Heap`
/// must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_seed_random_state(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_seed_random_state: expected 1 argument");
    }
    let seed = *args;
    encode(active_heap().alloc_random_state(seeded_random_state(seed)))
}

/// Decodes `args[idx]` as a boxed random-state.
///
/// # Safety
///
/// `args` must point to at least `idx + 1` valid `i64`s, `args[idx]`
/// decoding to a `Value::Boxed` random-state; a `Heap` must already be
/// registered on this thread.
unsafe fn random_state_arg(args: *const i64, idx: isize, who: &str) -> BoxId {
    match decode(*args.offset(idx)) {
        Value::Boxed(id) if active_heap().is_random_state(id) => id,
        _ => fatal(&format!("{who}: argument is not a random-state")),
    }
}

/// `random-state-next` for compiled code — advances `args[0]`'s seed one
/// xorshift step and returns the draw reduced into `[0, args[1])`, matching
/// the interpreter's own `eval_random_state_next` (`args[1]` is a bare
/// `i64` bound, and the result is a bare `i64` too). A non-positive bound
/// [`raise`]s that same function's message, so `(random 0)` fails
/// identically whichever tier the prelude's `random` is running on.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args[0]` must decode to a boxed random-state;
/// a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_random_state_next(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_random_state_next: expected 2 arguments");
    }
    let id = random_state_arg(args, 0, "rt_random_state_next");
    let bound = *args.add(1);
    if bound <= 0 {
        return raise(format!("random: bound must be positive, got {}", bound)).into_word();
    }
    let heap = active_heap();
    let next = xorshift64_step(heap.random_state_seed(id));
    heap.set_random_state_seed(id, next);
    (next % bound as u64) as i64
}

/// `random-state-copy` for compiled code — a new state starting where
/// `args[0]` is now, so later draws against either never affect the other
/// (`eval_random_state_copy`). The result is a fresh allocation, tagged and
/// as unrooted as [`rt_str_new`]'s until a caller protects it.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args[0]` must decode to a boxed random-state;
/// a `Heap` must already be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_random_state_copy(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_random_state_copy: expected 1 argument");
    }
    let id = random_state_arg(args, 0, "rt_random_state_copy");
    let heap = active_heap();
    let seed = heap.random_state_seed(id);
    encode(heap.alloc_random_state(seed))
}
