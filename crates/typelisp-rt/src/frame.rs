//! The coroutine frame protocol: opening a frame, reading the pending call's
//! arguments, and naming a call to the driver.

use typelisp_mem::prelude::*;
use typelisp_abi::fatal;
use typelisp_abi::call_state::{Pending, State};

/// `rt_frame_open(nslots, mask)` — everything a coroutine body does with the
/// frame it makes on entry, in one call: open it with `nslots` zeroed slots
/// and the function's mask table at address `mask` (null when no slot is
/// rooted) on the running task's root stack (`Heap::open_frame`), and publish
/// it to the driver. Returns the address of its words, which is what names
/// the frame from here on: the driver keeps it, and hands it back to the body
/// on every later activation.
///
/// It is made *inside* the callee, after the call has begun, so publishing it
/// here is the only moment the driver could be told of it. The mask is not
/// copied: which slots are rooted is the same for every activation of the
/// function.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to 2 valid `i64`s; a non-null
/// `mask` must point to `nslots.div_ceil(64)` words that outlive the frame. A
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_open(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_frame_open: expected 2 arguments (slot count, mask table)");
    }
    let (nslots, mask) = (*args, *args.add(1) as usize as *const u64);
    if nslots < typelisp_abi::FRAME_RESERVED_SLOTS as i64 {
        fatal(&format!("rt_frame_open: {} slots leave no room for the driver protocol's own", nslots));
    }
    typelisp_abi::call_state::with_heap(|heap, s| {
        let data = heap.open_frame(nslots as usize, mask).data() as i64;
        s.publish_frame(data);
        data
    })
}

/// `(rt-pending-argc)` for compiled code — how many arguments the driver left
/// for this entry.
///
/// # Safety
///
/// Takes no arguments; `args`/`argc` are ignored.
#[no_mangle]
pub unsafe extern "C" fn rt_pending_argc(_args: *const i64, _argc: u32) -> i64 {
    typelisp_abi::call_state::with(|s| s.pending_argc()) as i64
}

/// `(rt-pending-arg i)` for compiled code — one of the arguments the driver
/// left, as the raw word the caller stored.
///
/// `args[0]` is a raw index, not a value in the program.
///
/// # Safety
///
/// `argc` must be `>= 1`.
#[no_mangle]
pub unsafe extern "C" fn rt_pending_arg(args: *const i64, argc: u32) -> i64 {
    pending_word(args, argc, "rt_pending_arg", "argument", "an argument", State::pending_arg)
}

/// `(rt-pending-envc)` for compiled code — how many captures the driver left.
///
/// # Safety
///
/// Takes no arguments; `args`/`argc` are ignored.
#[no_mangle]
pub unsafe extern "C" fn rt_pending_envc(_args: *const i64, _argc: u32) -> i64 {
    typelisp_abi::call_state::with(|s| s.pending_envc()) as i64
}

/// `(rt-pending-env i)` for compiled code — one of the captures the driver
/// left, as the raw word the caller stored.
///
/// The capture counterpart of [`rt_pending_arg`], and separate for the same
/// reason the two lists are: a closure body reads its captures and its
/// parameters by separate indices.
///
/// # Safety
///
/// `argc` must be `>= 1`.
#[no_mangle]
pub unsafe extern "C" fn rt_pending_env(args: *const i64, argc: u32) -> i64 {
    pending_word(args, argc, "rt_pending_env", "capture", "a capture", State::pending_env)
}

/// The body of [`rt_pending_arg`] and [`rt_pending_env`]: word `args[0]` of
/// one of the driver's two lists, read by `get`, which says on a miss how
/// many the driver left. `what` names an entry of that list in the messages,
/// and `a_what` is the same name with its article.
unsafe fn pending_word(
    args: *const i64,
    argc: u32,
    who: &str,
    what: &str,
    a_what: &str,
    get: fn(&State, usize) -> Result<i64, usize>,
) -> i64 {
    if argc < 1 {
        fatal(&format!("{}: expected 1 argument", who));
    }
    let i = *args;
    if i < 0 {
        fatal(&format!("{}: {} is not {} index", who, i, a_what));
    }
    match typelisp_abi::call_state::with(|s| get(s, i as usize)) {
        Ok(w) => w,
        Err(count) => fatal(&format!("{}: {} {} was not passed (the driver left {})", who, what, i, count)),
    }
}

/// `(rt-frame-call target arg...)` for compiled code — names the call the
/// driver should make, immediately before the frame returns `STATUS_CALL`.
///
/// `args[0]` is the callee's address (`build-fn-address`); the rest are its
/// arguments, already evaluated and in the callee's declared representations.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to `argc` valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_call(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_frame_call: expected at least 1 argument (the callee)");
    }
    let target = *args as usize;
    name_call(args, argc, 1, |args| Pending::Direct { target, args, env: Vec::new() });
    0
}

/// Names a call to the driver: `args[from..argc]`, gathered into a reused
/// vector, become the arguments of the call `make` builds — in one access to
/// the thread's call state.
///
/// # Safety
///
/// `args` must point to `argc >= from` valid `i64`s.
unsafe fn name_call(args: *const i64, argc: u32, from: usize, make: impl FnOnce(Vec<i64>) -> Pending) {
    let words = core::slice::from_raw_parts(args.add(from), argc as usize - from);
    typelisp_abi::call_state::with(|s| {
        let mut passed = s.args_buffer(words.len());
        passed.extend_from_slice(words);
        s.set_pending_call(make(passed));
    });
}

/// `rt_frame_apply(closure, arg...)` — name a function *value* to the driver.
///
/// [`rt_frame_call`] for an `apply`, where the callee is not an address the
/// checker resolved but a value whose nature decides who runs it. The driver
/// resolves it ([`resolve_closure`]): a coroutine body joins this chain, a
/// classic one runs to completion, and an interpreted one goes to whoever
/// owns a continuation stack.
///
/// # Safety
///
/// `argc` must be `>= 1` and `args` must point to at least `argc` valid
/// `i64`s, the first a tagged function value and the rest its arguments in
/// its own declared representations.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_apply(args: *const i64, argc: u32) -> i64 {
    if argc < 1 {
        fatal("rt_frame_apply: expected at least 1 argument (the callee)");
    }
    let closure = *args;
    name_call(args, argc, 1, |args| Pending::Apply { closure, args });
    0
}

/// `rt_frame_dyn_call(vtable, slot, arg...)` — name a `:dyn` method to the
/// driver.
///
/// [`rt_frame_apply`] for a callee the vtable names. The arguments start with
/// the *concrete* receiver: the call site unwrapped the trait object before
/// storing it (`compile-dyn-call`), because the method behind the slot
/// expects its own type and knows nothing about the box.
///
/// # Safety
///
/// `argc` must be `>= 2` and `args` must point to at least `argc` valid
/// `i64`s: a vtable id, a slot index, and the method's arguments.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_dyn_call(args: *const i64, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_frame_dyn_call: expected at least 2 arguments (vtable id, slot)");
    }
    let vtable = *args as u32;
    let slot = *args.add(1) as u32;
    name_call(args, argc, 2, |args| Pending::Dyn { vtable, slot, args });
    0
}

/// `(rt-frame-call-env target env-ptr env-len arg...)` — [`rt_frame_call`] for
/// a callee that also has captures: a `lambda` body, or a `labels` sibling
/// reached under the captures-carrying calling convention.
///
/// `args[1]` is a pointer to `args[2]` captured words, read here and copied,
/// because the array it points at is on the caller's machine stack and the
/// caller is about to return.
///
/// # Safety
///
/// `argc` must be `>= 3`, `args` must point to `argc` valid `i64`s, and
/// `args[1]` must point to `args[2]` valid `i64`s.
#[no_mangle]
pub unsafe extern "C" fn rt_frame_call_env(args: *const i64, argc: u32) -> i64 {
    if argc < 3 {
        fatal("rt_frame_call_env: expected at least 3 arguments (callee, captures, capture count)");
    }
    let target = *args as usize;
    let env_ptr = *args.add(1) as usize as *const i64;
    let env_len = *args.add(2);
    if env_len < 0 {
        fatal(&format!("rt_frame_call_env: {} is not a capture count", env_len));
    }
    let mut env = Vec::with_capacity(env_len as usize);
    for i in 0..env_len as usize {
        env.push(*env_ptr.add(i));
    }
    name_call(args, argc, 3, |args| Pending::Direct { target, args, env });
    0
}
