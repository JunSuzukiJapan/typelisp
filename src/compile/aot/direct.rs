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
//! The prelude's functions the program can reach are judged with its own:
//! they are compiled into the same executable.
//!
//! The judgement errs one way only. A node it does not know is not looked
//! at, and the builders refuse to emit anything that stops in a direct
//! function — so a miss here is a build that fails, never a wrong one.
//!
//! A function whose address is handed to the runtime is a coroutine
//! whatever it does (`pinned`): the entry, a method in a vtable, a
//! registered `print-object`, an interrupt handler, a C callback. (On a
//! target with no runtime a handler is called by the vector's own code,
//! `avr::vector`, and is an ordinary function.) So is
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
#[derive(Clone)]
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
    /// The node that can stop it, if there is one.
    stops: Option<&'static str>,
    /// It has a loop.
    loops: bool,
    /// It makes another task wait or run.
    waits: bool,
    /// It reaches the interpreter.
    interprets: bool,
    /// The functions it calls by name.
    callees: Vec<String>,
    /// Every name in it, functions it only refers to included.
    names: Vec<String>,
}

/// The nodes that make a function a coroutine, and what each is in a message.
const STOPPING_NODES: [(&str, &str); 8] = [
    ("suspend", "an operation that waits"),
    ("select", "`select`"),
    ("apply", "a call of a `labels` function"),
    ("labels", "`labels`"),
    ("apply-indirect", "a call of a function value"),
    ("dyn-call", "a call through a `:dyn`"),
    ("catch", "`catch`"),
    ("unwind-protect", "`unwind-protect`"),
];

fn facts(heap: &Heap, form: Value) -> Facts {
    let mut f = Facts::default();
    let mut pending = vec![form];
    while let Some(v) = pending.pop() {
        match v {
            Value::Cons(_) => {
                let (Ok(head), Ok(rest)) = (heap.car(v), heap.cdr(v)) else { continue };
                if let Value::Symbol(tag) = head {
                    let name = tag.name();
                    if let Some((_, what)) = STOPPING_NODES.iter().find(|(node, _)| *node == name) {
                        f.stops = Some(what);
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
                f.names.push(s.to_string());
            }
            _ => {}
        }
    }
    f
}

/// What the entry's direct copy is called, after the entry's own symbol.
pub(super) const COPY_SUFFIX: &str = "$direct";

/// A build's direct functions ([`crate::compile::direct::producing`]), and
/// the functions to emit besides the program's own.
#[derive(Default)]
pub(crate) struct Direct {
    pub sigs: HashMap<String, DirectSig>,
    pub emit: Vec<Body>,
    /// The entry point's direct copy, when it has one.
    pub entry: Option<String>,
    /// What that copy, the static globals' initializers and the `handlers`
    /// can call, themselves included. Empty when there is no such copy.
    pub called: HashSet<String>,
    /// The program reaches the interpreter, which looks globals up in the
    /// runtime's table: none of them is static, and the initializers given as
    /// static ones are not emitted.
    pub interprets: bool,
    /// Why it has none, each with the functions from the entry to the one
    /// that is the reason. Empty when it has one.
    pub why_not: Vec<String>,
}

/// The direct functions of the program whose own functions are `bodies`,
/// whose static globals' initializers are `static_inits`, each a function of
/// its own, and whose other globals' initializers are `inits`. `handlers` are
/// the functions a target with no runtime enters besides `entry`. `module` holds the FFI thunks,
/// all of them coroutines; `prelude` translates the prelude function of a
/// symbol, if it is one.
pub(crate) fn direct_functions(
    heap: &mut Heap,
    module: &Module<'static>,
    bodies: &[Body],
    static_inits: &[Body],
    inits: &[Value],
    pinned: &HashSet<String>,
    entry: &str,
    handlers: &[String],
    mut prelude: impl FnMut(&mut Heap, &str) -> Result<Option<Body>, String>,
) -> Result<Direct, String> {
    let mut known: HashMap<String, Facts> =
        bodies.iter().chain(static_inits).map(|b| (b.symbol.clone(), facts(heap, b.body))).collect();
    let init_facts: Vec<Facts> = inits.iter().map(|f| facts(heap, *f)).collect();

    // The prelude functions the program refers to, and those those refer to.
    let mut answer = Direct::default();
    let mut pending: Vec<String> = known.values().chain(&init_facts).flat_map(|f| f.names.iter()).cloned().collect();
    pending.sort();
    let mut seen: HashSet<String> = known.keys().cloned().collect();
    while let Some(name) = pending.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(body) = prelude(heap, &name)? else { continue };
        let f = facts(heap, body.body);
        pending.extend(f.names.iter().cloned());
        known.insert(name, f);
        answer.emit.push(body);
    }

    // What the program does, with what it reaches.
    let all = || known.values().chain(&init_facts);
    if all().any(|f| f.interprets) {
        answer.interprets = true;
        answer.why_not.push("`eval` and the like, which need the interpreter".to_string());
        return Ok(answer);
    }
    let waits = all().any(|f| f.waits);

    let mut direct: HashSet<&str> = known
        .iter()
        .filter(|(name, f)| !pinned.contains(*name) && f.stops.is_none() && !(f.loops && waits))
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

    // The entry is a coroutine for the runtime to drive, and a direct
    // function for a `main` that needs none of it.
    let e = &known[entry];
    if e.stops.is_none() && !(e.loops && waits) && e.callees.iter().all(|c| direct.contains(c.as_str()) || (!known.contains_key(c) && module.get_function(c).is_none())) {
        let body = bodies.iter().find(|b| b.symbol == entry).expect("the entry is one of the program's functions");
        let copy = format!("{}{}", entry, COPY_SUFFIX);
        let mut pending = vec![entry];
        let roots = static_inits.iter().map(|b| b.symbol.as_str()).chain(handlers.iter().map(String::as_str));
        for root in roots.filter(|s| direct.contains(s)) {
            answer.called.insert(root.to_string());
            pending.push(root);
        }
        while let Some(at) = pending.pop() {
            for callee in known.get(at).into_iter().flat_map(|f| &f.callees) {
                if direct.contains(callee.as_str()) && answer.called.insert(callee.clone()) {
                    pending.push(callee);
                }
            }
        }
        answer.called.insert(copy.clone());
        answer.sigs.insert(copy.clone(), DirectSig::of(&body.params, &body.ret));
        answer.emit.push(Body { symbol: copy.clone(), ..body.clone() });
        answer.entry = Some(copy);
    } else {
        answer.why_not = why_not_direct(&known, module, waits, entry);
    }
    let roots = static_inits.iter().map(|b| b.symbol.as_str()).chain(handlers.iter().map(String::as_str));
    for root in roots.filter(|s| *s != entry && !direct.contains(s)) {
        answer.why_not.extend(why_not_direct(&known, module, waits, root));
    }
    answer.emit.extend(static_inits.iter().cloned());
    let sigs: Vec<(String, DirectSig)> = bodies
        .iter()
        .chain(&answer.emit)
        .filter(|b| direct.contains(b.symbol.as_str()))
        .map(|b| (b.symbol.clone(), DirectSig::of(&b.params, &b.ret)))
        .collect();
    answer.sigs.extend(sigs);
    Ok(answer)
}

/// What makes `entry` a coroutine: every function it can call that is one by
/// what it does itself, with the calls that lead to it.
fn why_not_direct(known: &HashMap<String, Facts>, module: &Module<'static>, waits: bool, entry: &str) -> Vec<String> {
    let mut whys = Vec::new();
    let mut seen: HashSet<&str> = HashSet::from([entry]);
    let mut pending: std::collections::VecDeque<Vec<&str>> = [vec![entry]].into();
    while let Some(chain) = pending.pop_front() {
        let at = chain[chain.len() - 1];
        let mut because = |what: &str| {
            let chain: Vec<&str> = chain.iter().map(|s| typelisp_mem::ConcreteNames::link_name_of(s).unwrap_or(s)).collect();
            whys.push(format!("{} (in {})", what, chain.join(" -> ")));
        };
        let Some(f) = known.get(at) else {
            // A name that is no function at all is a method computed in place.
            if module.get_function(at).is_some() {
                because("a C function");
            }
            continue;
        };
        if let Some(what) = f.stops {
            because(what);
        }
        if f.loops && waits {
            because("a loop, in a program with tasks");
        }
        if calls_itself(known, at) {
            because("a function that calls itself");
        }
        for callee in &f.callees {
            if seen.insert(callee) {
                pending.push_back(chain.iter().copied().chain([callee.as_str()]).collect());
            }
        }
    }
    whys
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
