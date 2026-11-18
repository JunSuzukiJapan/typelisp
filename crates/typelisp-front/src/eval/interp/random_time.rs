//! `random-state` and the time built-ins.

use crate::{BoxId, Heap, Value};
use crate::eval::value::EvalError;
use super::rt_i64;

/// One step of a 64-bit xorshift generator — the bit-twiddling behind every
/// `random-state` draw. Not cryptographically secure and not expressible in
/// typelisp itself (no bitwise operators), matching `gensym`'s
/// "collision-resistant, not unforgeable" precedent for what a builtin
/// without a real entropy/hygiene API can promise.
///
/// Lives in `typelisp-rt` (with the compiled lowering,
/// `rt_random_state_next`) rather than here, and is used from both sides: a
/// draw must not depend on whether the caller happened to be compiled.
use typelisp_rt::xorshift64_step;

/// A fresh entropy seed for `make-random-state-fresh`. Lives beside the
/// compiled lowering (`rt_make_random_state_fresh`), like
/// [`xorshift64_step`].
use typelisp_rt::fresh_random_seed;

/// The box behind a `random-state` value. A *positive* `is_random_state` test,
/// so a differently-shaped box (a float, a struct) is reported rather than
/// read as a seed by the accessors below, which panic on a mismatch.
fn expect_random_state(heap: &Heap, v: &Value) -> Result<BoxId, EvalError> {
    match v {
        Value::Boxed(id) if heap.is_random_state(*id) => Ok(*id),
        other => Err(EvalError::Internal(format!("expected a random-state, got {:?}", other))),
    }
}

/// `make-random-state-fresh`: a brand new, independently-seeded stream —
/// every `(defvar *random-state* ...)` in the prelude gets one at load time,
/// and it backs CL's `(make-random-state t)` case.
pub(super) fn eval_make_random_state_fresh(heap: &mut Heap, _args: &[Value]) -> Result<Value, EvalError> {
    Ok(heap.alloc_random_state(fresh_random_seed()))
}

/// The integer-to-state map a user-supplied seed goes through. Lives in
/// `typelisp-rt` beside the compiled lowering (`rt_seed_random_state`), like
/// [`xorshift64_step`] and [`fresh_random_seed`], so a seeded run is the same
/// run whichever tier it executes on.
use typelisp_rt::seeded_random_state;

/// `seed-random-state`: the stream a given integer names. Unlike
/// `make-random-state-fresh` this is reproducible — the point of it — so the
/// mapping must be stable across releases, which is why it is written down
/// once in [`seeded_random_state`] rather than inlined here.
pub(super) fn eval_seed_random_state(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let seed = rt_i64(&args[0])?;
    Ok(heap.alloc_random_state(seeded_random_state(seed)))
}

/// `random-state-copy`: an independent stream starting from the same point
/// `state` is at right now — CL's `(make-random-state state)` case. Later
/// draws against the copy never affect `state` (or vice versa) — distinct
/// `Rc`s over distinct `Cell`s, not a second handle to the same one.
pub(super) fn eval_random_state_copy(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_random_state(heap, &args[0])?;
    let seed = heap.random_state_seed(id);
    Ok(heap.alloc_random_state(seed))
}

/// `random-state-next`: advances `state` one xorshift step and returns the
/// draw reduced into `[0, bound)`. The prelude's `random` (an ordinary
/// `&optional`-taking `defun`) is the only caller — this is the one place
/// that actually touches a `random-state`'s seed.
pub(super) fn eval_random_state_next(heap: &mut Heap, args: &[Value]) -> Result<Value, EvalError> {
    let id = expect_random_state(heap, &args[0])?;
    let n = rt_i64(&args[1])?;
    if n <= 0 {
        return Err(EvalError::Panic(format!("random: bound must be positive, got {}", n)));
    }
    let next = xorshift64_step(heap.random_state_seed(id));
    heap.set_random_state_seed(id, next);
    Ok(Value::Int((next % n as u64) as i64))
}

/// `get-universal-time` (CLHS 25.1): seconds since 1900-01-01 00:00:00 UTC
/// (CL's epoch) — the Unix epoch offset by the well-known 2208988800s
/// between the two.
pub(super) fn eval_get_universal_time(heap: &mut Heap, _args: &[Value]) -> Result<Value, EvalError> {
    let now = typelisp_rt::sys_builtin::get_universal_time();
    Ok(typelisp_rt::sys_builtin::universal_time_value(heap, now))
}

/// `get-internal-real-time` (CLHS 25.1): elapsed `internal-time-units-per-
/// second` (the prelude's `defvar`, 1_000_000 — i.e. microseconds) since an
/// arbitrary reference point fixed at first call — a monotonic
/// `std::time::Instant`, not wall-clock time, so `time`'s elapsed-time
/// measurement can't go backwards under a clock adjustment.
pub(super) fn eval_get_internal_real_time(heap: &mut Heap, _args: &[Value]) -> Result<Value, EvalError> {
    let now = typelisp_rt::sys_builtin::get_internal_real_time();
    Ok(typelisp_rt::sys_builtin::internal_time_value(heap, now))
}

pub(super) fn eval_get_internal_run_time(heap: &mut Heap, _args: &[Value]) -> Result<Value, EvalError> {
    let used = typelisp_rt::sys_builtin::get_internal_run_time();
    Ok(typelisp_rt::sys_builtin::internal_time_value(heap, used))
}

/// The two `i32` arguments of the time-zone builtins. Unreachable through
/// the checker, so the error is the internal-shape one every other builtin
/// argument mismatch here reports.
pub(super) fn int_pair(args: &[Value], who: &str) -> Result<(i64, i64), EvalError> {
    match (args.first(), args.get(1)) {
        (Some(Value::Int(a)), Some(Value::Int(b))) => Ok((*a, *b)),
        other => Err(EvalError::Panic(format!("{}: arguments are not two integers, got {:?}", who, other))),
    }
}
