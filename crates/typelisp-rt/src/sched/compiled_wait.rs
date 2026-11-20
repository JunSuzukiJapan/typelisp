//! Turning a compiled task's suspension back into the `Waiting` it parks on.

use typelisp_mem::{Heap, TypeId, Value};
use super::{chan_id_of, task_id_of, ChanOp, SchedError, SelectOp, Waiting, Wake};

// ---- turning a compiled suspension back into a wait ------------------------

/// `(net-wait h interest)` — what to park **this task** on: the socket
/// under handle `h`, for reading (`0`) or writing (`1`).
///
/// The handle is turned into a descriptor here, once, for both the
/// interpreted call and the compiled suspension (`pending_wait`), so the
/// two refuse the same handles in the same words. A closed or non-socket
/// handle is a program error — the stream was closed and then waited on —
/// and panics rather than parking a task nothing will ever wake.
pub fn io_wait(heap: &mut Heap, argv: &[Value], deadline: Option<typelisp_mem::host::Instant>) -> Result<Waiting, SchedError> {
    let handle = match argv.first() {
        Some(Value::Int(h)) => *h,
        other => return Err(SchedError::Internal(format!("net-wait: {:?} is not a stream handle", other))),
    };
    let interest = match argv.get(1) {
        Some(Value::Int(code)) => crate::os::Interest::from_code(*code).ok_or_else(|| {
            SchedError::Internal(format!("net-wait: {} is not an interest (0 readable, 1 writable)", code))
        })?,
        other => return Err(SchedError::Internal(format!("net-wait: {:?} is not an interest", other))),
    };
    let fd = crate::stream::with_streams(heap, |t| t.raw_fd(handle)).map_err(SchedError::Panic)?;
    Ok(Waiting::Io { fd, interest, deadline })
}

/// `secs` from now, with `sleep`'s refusals: negative or NaN is not a
/// duration, and neither is one the clock cannot hold.
pub fn io_deadline(secs: f64) -> Result<typelisp_mem::host::Instant, SchedError> {
    if !(secs >= 0.0) {
        return Err(SchedError::Panic(format!("net-wait-for: {} is not a non-negative number of seconds", secs)));
    }
    let d = std::time::Duration::try_from_secs_f64(secs)
        .map_err(|_| SchedError::Panic(format!("net-wait-for: {} is longer than this can wait", secs)))?;
    Ok(typelisp_mem::host::Instant::now() + d)
}

/// `(sleep secs)` as a wait, with the refusals CL makes: a negative or NaN
/// wait is an error, and there is no duration to build from one. The same
/// words on both sides, because a program must not be able to tell which
/// side its `sleep` ran on.
pub fn sleep_wait(secs: f64) -> Result<Waiting, SchedError> {
    if !(secs >= 0.0) {
        return Err(SchedError::Panic(format!("sleep: {} is not a non-negative number of seconds", secs)));
    }
    let d = std::time::Duration::try_from_secs_f64(secs)
        .map_err(|_| SchedError::Panic(format!("sleep: {} is longer than this can wait", secs)))?;
    Ok(Waiting::Until(typelisp_mem::host::Instant::now() + d))
}

/// The type a compiled suspension site appended as its second word — the
/// bridge put it there, because the operation's own word does not name the
/// type the answer is built as. `what` is the operation and `ty` the type it
/// should be, for the message a malformed site gets.
fn key_word(word: i64, what: &str, ty: &str) -> Result<TypeId, SchedError> {
    TypeId::from_word(word).ok_or_else(|| {
        SchedError::Internal(format!("a compiled {} carried {} where its {} type should be", what, word, ty))
    })
}

/// What a compiled frame that just returned `STATUS_SUSPEND` is waiting for,
/// and how its answer will come back.
///
/// The compiled side publishes two raw words (`typelisp_abi::call_state`),
/// because the crate it publishes from sits below this one — a `TaskId` and
/// an `Instant` are the scheduler's types. This is the one place that turns
/// them back.
pub fn pending_wait(heap: &mut Heap) -> Result<(Waiting, Wake), SchedError> {
    use typelisp_abi::call_state as cs;
    let (kind, payload, second) = cs::take_pending_suspend().ok_or_else(|| {
        SchedError::Internal("a compiled frame suspended without recording what it was waiting for".to_string())
    })?;
    match kind {
        // Nothing comes back: the wake value is unit, and the resume block
        // does not read the frame's value slot at all (`(suspend ...)` carries
        // kind `0` for these).
        cs::SUSPEND_YIELD => Ok((Waiting::Yield, Wake::Unit)),
        // A compiled loop's back edge offering a turn (C7). Indistinguishable
        // from `(yield)` once a scheduler is the one answering — the two are
        // separate kinds so that a driver *without* a scheduler can tell an
        // offer from a request.
        cs::SUSPEND_SAFEPOINT => Ok((Waiting::Yield, Wake::Unit)),
        cs::SUSPEND_SLEEP => Ok((sleep_wait(f64::from_bits(payload as u64))?, Wake::Unit)),
        // The payload is the `Task<T>` handle, still tagged — reading the id
        // out of the box is this side's job, because `TaskId` is the
        // scheduler's type. The answer comes back as a tagged `Sexpr` whatever
        // `T` is, and the resume block decodes it with the kind the bridge
        // baked in: the same division `catch`/`throw` make for a thrown value.
        cs::SUSPEND_WAIT => {
            let handle = typelisp_abi::decode(payload);
            Ok((Waiting::Task(task_id_of(heap, Some(handle))?), Wake::Tagged))
        }
        // Both words raw: a stream handle and an interest code are plain
        // integers in compiled code (`sleep`'s bits cross the same way), and
        // this is the same resolution the interpreted `net-wait` makes.
        cs::SUSPEND_IO => {
            let w = io_wait(heap, &[Value::Int(payload), Value::Int(second)], None)?;
            Ok((w, Wake::Unit))
        }
        // The handle and the interest share the first word (`rt_suspend_io_for`
        // packs them: handle above, interest in the low bit) so the seconds'
        // bits can have the second. The answer is a `bool`, tagged like
        // every typed wake value.
        cs::SUSPEND_IO_FOR => {
            let d = io_deadline(f64::from_bits(second as u64))?;
            let w = io_wait(heap, &[Value::Int(payload >> 1), Value::Int(payload & 1)], Some(d))?;
            Ok((w, Wake::Tagged))
        }
        // The channel operations. The handle is tagged, like `wait`'s; so is
        // `send`'s value, which the suspension site tagged per the element's
        // own `Repr::field_kind` because the driver has only a word.
        // The capacity is an ordinary `i32` argument, so it crosses raw — the
        // suspension site tags only what the driver could not otherwise read.
        // The second word is the channel's type, which the bridge appended
        // at the site (as `recv`'s `Option<T>` is).
        cs::SUSPEND_CHAN_NEW => {
            let key = key_word(second, "`Chan::new`", "`Chan<T>`")?;
            Ok((Waiting::Chan(ChanOp::New(payload, key)), Wake::Tagged))
        }
        // The closure is tagged, and the answer — the handle — comes back
        // tagged like every other typed wake value. The second word is the
        // handle's type, appended by the bridge.
        cs::SUSPEND_TASK => {
            let key = key_word(second, "`task`", "`Task<T>`")?;
            Ok((Waiting::Spawn(typelisp_abi::decode(payload), key), Wake::Tagged))
        }
        cs::SUSPEND_THREAD => {
            let key = key_word(second, "`thread`", "`Thread<T>`")?;
            Ok((Waiting::SpawnThread(typelisp_abi::decode(payload), key), Wake::Tagged))
        }
        cs::SUSPEND_MAIN => Ok((Waiting::Main, Wake::Unit)),
        // `Wake::Tagged` for every typed answer: a wake value always crosses
        // back **tagged**, whatever its type, and the resume block decodes it
        // with the kind the bridge baked in — the same division `wait` makes.
        cs::SUSPEND_CHAN_LEN => {
            Ok((Waiting::Chan(ChanOp::Len(chan_id_of(heap, Some(typelisp_abi::decode(payload)))?)), Wake::Tagged))
        }
        cs::SUSPEND_CHAN_CAP => {
            Ok((Waiting::Chan(ChanOp::Cap(chan_id_of(heap, Some(typelisp_abi::decode(payload)))?)), Wake::Tagged))
        }
        cs::SUSPEND_CHAN_CLOSE => {
            Ok((Waiting::Chan(ChanOp::Close(chan_id_of(heap, Some(typelisp_abi::decode(payload)))?)), Wake::Unit))
        }
        cs::SUSPEND_CHAN_SEND => {
            let c = chan_id_of(heap, Some(typelisp_abi::decode(payload)))?;
            Ok((Waiting::Chan(ChanOp::Send(c, typelisp_abi::decode(second))), Wake::Unit))
        }
        cs::SUSPEND_CHAN_RECV => {
            let c = chan_id_of(heap, Some(typelisp_abi::decode(payload)))?;
            let key = key_word(second, "`recv`", "`Option<T>`")?;
            Ok((Waiting::Chan(ChanOp::Recv(c, key)), Wake::Tagged))
        }
        // The arms travel in their own slot, and every word in them is
        // tagged — the descriptor was built in a compiled frame's slots, and
        // the collector walks those.
        cs::SUSPEND_CHAN_SELECT => {
            let words = cs::take_pending_select();
            let untag = |w: i64| w >> typelisp_mem::tagged::FIXNUM_SHIFT;
            if words.len() < 2 {
                return Err(SchedError::Internal("a compiled `select` carried no arms".to_string()));
            }
            let n = usize::try_from(untag(words[0])).map_err(|_| {
                SchedError::Internal(format!("a compiled `select` said it had {} arms", untag(words[0])))
            })?;
            let has_else = untag(words[1]) != 0;
            if words.len() < 2 + 3 * n {
                return Err(SchedError::Internal(format!(
                    "a compiled `select` said it had {} arms and carried {} words",
                    n,
                    words.len()
                )));
            }
            let mut ops = Vec::with_capacity(n);
            for i in 0..n {
                let base = 2 + 3 * i;
                let c = chan_id_of(heap, Some(typelisp_abi::decode(words[base + 1])))?;
                ops.push(if untag(words[base]) == 0 {
                    SelectOp::Recv(c, key_word(words[base + 2], "`select` receive arm", "`Option<T>`")?)
                } else {
                    SelectOp::Send(c, typelisp_abi::decode(words[base + 2]))
                });
            }
            Ok((Waiting::Select { ops, has_else }, Wake::Tagged))
        }
        other => Err(SchedError::Internal(format!(
            "a compiled frame asked to wait on kind {} (payload {}), which this build does not lower",
            other, payload
        ))),
    }
}
