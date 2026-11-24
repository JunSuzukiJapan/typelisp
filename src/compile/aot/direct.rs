//! Which of an executable's functions are direct (`crate::compile::direct`).
//!
//! A function is direct when nothing it does can stop it in the middle, and
//! everything it calls is direct too. Read off each function's island IR,
//! before any is emitted, because a function's shape decides how every call
//! to it is emitted.
//!
//! What can stop a function, by the IR node that does it:
//!
//! * a suspension (`suspend`, `select`) — a task, a channel, `sleep`, a move
//!   to the interpreter's thread;
//! * a call through a value (`apply-indirect`, `dyn-call`) or to a `labels`
//!   function (`apply`, `labels`): those callees are coroutines, and so is
//!   anything a function value can hold;
//! * a region that catches (`catch`, `unwind-protect`): the driver is what
//!   resumes a frame at its handler;
//! * a loop, in a program that runs another task: the loop's safepoint
//!   offers the scheduler a turn;
//! * a call that can come back to the function itself: a direct call uses
//!   the machine stack, and how deep a recursion goes is limited by the
//!   heap, not by the stack.
//!
//! A prelude function the program calls is a coroutine in the prelude's
//! bitcode. One that could be direct is also emitted as a direct function
//! under a name of its own ([`COPY_SUFFIX`]), which the program's own code
//! calls; the rest of the prelude goes on calling the coroutine.
//!
//! The judgement errs one way only. A node it does not know is not looked
//! at, and the builders refuse to emit anything that stops in a direct
//! function — so a miss here is a build that fails, never a wrong one.
//!
//! A function whose address is handed to the runtime is a coroutine
//! whatever it does (`pinned`): the entry, a method in a vtable, a
//! registered `print-object`, an interrupt handler, a C callback. So is
//! every function of a program that can reach the interpreter, which may
//! call any of them by name. [`check_only_called`] makes sure, after the
//! executable's `main` is built, that nothing took a direct function's
//! address.

use std::collections::{HashMap, HashSet};

use inkwell::module::Module;
use inkwell::values::AsValueRef;

use typelisp_mem::{Heap, Value};

use crate::check::repr::Repr;
use crate::compile::direct::DirectSig;

/// One function of the executable, translated for the island.
pub(crate) struct Body {
    pub symbol: String,
    pub params: Vec<Repr>,
    pub ret: Repr,
    pub param_list: Value,
    pub body: Value,
}

/// What one function's IR says about it.
#[derive(Default)]
struct Facts {
    /// Something in it can stop it.
    stops: bool,
    /// It has a loop.
    loops: bool,
    /// It makes another task wait or run.
    waits: bool,
    /// It reaches the interpreter.
    interprets: bool,
    /// The functions it calls by name.
    callees: Vec<String>,
}

/// The nodes that make a function a coroutine.
const STOPPING_NODES: [&str; 8] = ["suspend", "select", "apply", "labels", "apply-indirect", "dyn-call", "catch", "unwind-protect"];

fn facts(heap: &Heap, form: Value) -> Facts {
    let mut f = Facts::default();
    let mut pending = vec![form];
    while let Some(v) = pending.pop() {
        match v {
            Value::Cons(_) => {
                let (Ok(head), Ok(rest)) = (heap.car(v), heap.cdr(v)) else { continue };
                if let Value::Symbol(tag) = head {
                    let name = tag.name();
                    if STOPPING_NODES.contains(&name) {
                        f.stops = true;
                    }
                    if name == "loop" {
                        f.loops = true;
                    }
                    let callee_at = match name {
                        "call" => Some(0),
                        "assoc" => Some(3),
                        _ => None,
                    };
                    if let Some(Value::Str(id)) = callee_at.and_then(|i| heap.list_to_vec(rest).ok()?.get(i).copied()) {
                        let callee = heap.string(id).to_string();
                        if !callee.starts_with("rt_") {
                            f.callees.push(callee);
                        }
                    }
                }
                pending.push(head);
                pending.push(rest);
            }
            Value::Str(id) => {
                let s = heap.string(id).to_string();
                let s = s.as_str();
                if crate::compile::externs::needs_scheduler(s) {
                    f.waits = true;
                }
                if crate::compile::externs::needs_interpreter(s) || s == crate::compile::externs::RT_SUSPEND_MAIN {
                    f.interprets = true;
                }
            }
            _ => {}
        }
    }
    f
}

/// What a prelude function's direct copy is called, after the function's own
/// symbol.
pub(super) const COPY_SUFFIX: &str = "$direct";

/// A build's direct functions ([`crate::compile::direct::producing`]), and
/// the prelude functions among them to emit, each already under its copy's
/// symbol.
#[derive(Default)]
pub(crate) struct Direct {
    pub sigs: HashMap<String, DirectSig>,
    pub copies: HashMap<String, String>,
    pub emit: Vec<Body>,
}

/// The direct functions of the program whose own functions are `bodies` and
/// whose `defvar`s' initializers are `inits`. `module` holds everything else
/// they can call (the prelude, the FFI thunks), all of it coroutines;
/// `prelude` translates the prelude function of a symbol, if it is one.
pub(crate) fn direct_functions(
    heap: &mut Heap,
    module: &Module<'static>,
    bodies: &[Body],
    inits: &[Value],
    pinned: &HashSet<String>,
    mut prelude: impl FnMut(&mut Heap, &str) -> Result<Option<Body>, String>,
) -> Result<Direct, String> {
    let mut known: HashMap<String, Facts> = bodies.iter().map(|b| (b.symbol.clone(), facts(heap, b.body))).collect();
    let init_facts: Vec<Facts> = inits.iter().map(|f| facts(heap, *f)).collect();

    // What the program does, and what the functions outside it that it calls
    // can reach.
    let all = || known.values().chain(&init_facts);
    let mut waits = all().any(|f| f.waits);
    let interprets = all().any(|f| f.interprets);
    let outside: HashSet<String> = all().flat_map(|f| f.callees.iter()).filter(|c| !known.contains_key(*c)).cloned().collect();
    let reaches = |names: &HashSet<String>, shim: fn(&str) -> bool| {
        names
            .iter()
            .filter_map(|name| module.get_function(name))
            .any(|f| !super::reach::uses(module, f, |n| shim(n).then(String::new)).is_empty())
    };
    if interprets || reaches(&outside, crate::compile::externs::needs_interpreter) {
        return Ok(Direct::default());
    }
    waits |= reaches(&outside, crate::compile::externs::needs_scheduler);

    // The prelude functions they call, and those those call, as far as one
    // that stops.
    let mut copied: Vec<Body> = Vec::new();
    let mut pending: Vec<String> = outside.into_iter().collect();
    pending.sort();
    let mut seen: HashSet<String> = pending.iter().cloned().collect();
    while let Some(name) = pending.pop() {
        let Some(body) = prelude(heap, &name)? else { continue };
        let f = facts(heap, body.body);
        if !f.stops {
            for callee in &f.callees {
                if !known.contains_key(callee) && seen.insert(callee.clone()) {
                    pending.push(callee.clone());
                }
            }
        }
        known.insert(name, f);
        copied.push(body);
    }

    let mut direct: HashSet<&str> = known
        .iter()
        .filter(|(name, f)| !pinned.contains(*name) && !f.stops && !(f.loops && waits))
        .map(|(name, _)| name.as_str())
        .collect();
    let cyclic: HashSet<&str> = direct.iter().copied().filter(|name| calls_itself(&known, name)).collect();
    direct.retain(|name| !cyclic.contains(name));
    // A callee that is not direct makes its caller a coroutine. A name that
    // is neither one of these nor in the module is no function at all: a
    // method the island computes in place.
    loop {
        let still: HashSet<&str> = direct
            .iter()
            .copied()
            .filter(|name| known[*name].callees.iter().all(|c| direct.contains(c.as_str()) || (!known.contains_key(c) && module.get_function(c).is_none())))
            .collect();
        if still.len() == direct.len() {
            break;
        }
        direct = still;
    }

    let mut answer = Direct::default();
    for b in bodies.iter().filter(|b| direct.contains(b.symbol.as_str())) {
        answer.sigs.insert(b.symbol.clone(), DirectSig::of(&b.params, &b.ret));
    }
    for mut b in copied.into_iter().filter(|b| direct.contains(b.symbol.as_str())) {
        let copy = format!("{}{}", b.symbol, COPY_SUFFIX);
        answer.sigs.insert(copy.clone(), DirectSig::of(&b.params, &b.ret));
        answer.copies.insert(std::mem::replace(&mut b.symbol, copy.clone()), copy);
        answer.emit.push(b);
    }
    Ok(answer)
}

/// Whether a call of `name` can come back to `name`.
fn calls_itself(known: &HashMap<String, Facts>, name: &str) -> bool {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut pending: Vec<&str> = vec![name];
    while let Some(at) = pending.pop() {
        for callee in known.get(at).into_iter().flat_map(|f| &f.callees) {
            if callee == name {
                return true;
            }
            if seen.insert(callee) {
                pending.push(callee);
            }
        }
    }
    false
}

/// Every use of each direct function in `module` is a call of it — so none
/// of them can be entered under the coroutine ABI.
pub(crate) fn check_only_called(module: &Module<'static>, direct: &[String]) -> Result<(), String> {
    use inkwell::llvm_sys::core::{LLVMGetCalledValue, LLVMGetFirstUse, LLVMGetNextUse, LLVMGetUser, LLVMIsACallInst};
    for name in direct {
        let Some(f) = module.get_function(name) else { continue };
        let fv = f.as_value_ref();
        // SAFETY: walking the use list of a live function in a live module.
        unsafe {
            let mut u = LLVMGetFirstUse(fv);
            while !u.is_null() {
                let user = LLVMGetUser(u);
                // The address the call site was named by (`build-fn-address`),
                // a constant nothing uses once the call is made directly.
                let dead_constant = !inkwell::llvm_sys::core::LLVMIsAConstant(user).is_null() && LLVMGetFirstUse(user).is_null();
                if dead_constant {
                    u = LLVMGetNextUse(u);
                    continue;
                }
                if LLVMIsACallInst(user).is_null() || LLVMGetCalledValue(user) != fv {
                    let printed = inkwell::llvm_sys::core::LLVMPrintValueToString(user);
                    let text = std::ffi::CStr::from_ptr(printed).to_string_lossy().into_owned();
                    inkwell::llvm_sys::core::LLVMDisposeMessage(printed);
                    return Err(format!("internal error: the direct function `{}` is used other than by calling it: {}", name, text.trim()));
                }
                u = LLVMGetNextUse(u);
            }
        }
    }
    Ok(())
}
