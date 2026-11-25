//! C calling back into typelisp: the entries C is handed, and the runtime
//! half of every call C makes through one.
//!
//! An **entry** is a function with the C signature a `defffi` declared for a
//! callback parameter. Its body is emitted by the backend (`compile::ffi`):
//! it leaves the native section the C call put this thread in
//! ([`rt_ffi_callback_enter`]), turns C's arguments into words, runs the
//! typelisp function to completion ([`rt_ffi_callback_invoke`]), turns the
//! answer back and re-enters the native section ([`rt_ffi_callback_leave`]).
//!
//! Entries are kept in a table keyed by the function's definition (its module
//! and its symbol) and the function type C calls it at. An executable fills it
//! at startup
//! ([`rt_ffi_callback_register`]); a session fills it on first use through
//! the factory the front end installs ([`set_factory`]).
//!
//! # Failures wait for C to return
//!
//! A `panic` or an uncaught `throw` in a callback is raised, and C's frames
//! sit between the callback and whoever could catch it. So the failure is
//! kept on this thread and C is handed a zero; any further callback while it
//! is kept returns zero at once without running; and the FFI thunk raises it
//! again the moment the C function returns ([`resume_pending`], from
//! `rt_ffi_leave_native`).

use typelisp_mem::Word;
use typelisp_mem::prelude::*;
use core::any::Any;
use core::cell::RefCell;
use typelisp_mem::collections::HashMap;
use typelisp_mem::sync::{Mutex, OnceLock};

use typelisp_abi::{active_heap, fatal, raise, Raised};
use typelisp_mem::{NsId, SymRef, TypeId, Value};

use crate::decode;

/// A callback: the definition of the function C is handed, and the function
/// type it is called at.
pub type Callback = (NsId, SymRef, TypeId);

/// Entry addresses by callback, as an executable registered them at startup. A
/// process-wide table: an entry is plain code, callable from any thread that
/// runs typelisp, and never removed — C may keep the address for as long as
/// the process lives. A session keeps the entries it makes itself, per
/// interpreter (they run code that session compiled), and asks [`FACTORY`].
static ENTRIES: Mutex<Option<HashMap<Callback, usize>>> = Mutex::new(None);

/// What makes an entry nobody registered: set by a session that can compile
/// one (the JIT), absent in an executable, where every entry was registered
/// at startup.
static FACTORY: OnceLock<fn(Callback) -> Result<usize, String>> = OnceLock::new();

typelisp_mem::thread_local! {
    /// A callback's failure, waiting for the C call around it to return —
    /// see the module comment.
    static PENDING: RefCell<Option<Box<dyn Any + Send>>> = const { RefCell::new(None) };
}

/// The entry registered for `callback`, if any.
pub fn lookup(callback: Callback) -> Option<usize> {
    let guard = ENTRIES.lock();
    guard.as_ref().and_then(|m| m.get(&callback).copied())
}

/// Records the entry for `callback` — an executable's, at startup.
pub fn register(callback: Callback, addr: usize) {
    let mut guard = ENTRIES.lock();
    guard.get_or_insert_with(HashMap::new).insert(callback, addr);
}

/// Installs the factory [`rt_ffi_callback_address`] asks for an entry nobody
/// registered. The first installation wins; every session installs the same
/// function.
pub fn set_factory(f: fn(Callback) -> Result<usize, String>) {
    let _ = FACTORY.set(f);
}

/// Raises again a callback's failure kept while C was running, if there is
/// one. Called by `rt_ffi_leave_native` once the C function has returned and
/// this thread is running typelisp again.
pub fn resume_pending() -> Result<(), Raised> {
    match PENDING.with(|p| p.borrow_mut().take()) {
        Some(payload) => Err(typelisp_abi::raise_payload(payload)),
        None => Ok(()),
    }
}

/// `(" ffi-callback-address" PATH TYPE)`: the address of the entry for the
/// function the path `path` names, called at the function type `ty`,
/// as a raw word.
///
/// # Safety
///
/// The words must be valid, and a `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_ffi_callback_address(path: Word, ty: Word) -> Word {
    let (ns, sym) = match decode(path) {
        Value::Symbol(sym) => (sym.home(), sym),
        other => fatal(&format!("rt_ffi_callback_address: the function is not a symbol, got {:?}", other)),
    };
    let ty = TypeId::from_word(ty)
        .unwrap_or_else(|| fatal("rt_ffi_callback_address: the function type is not a type"));
    if let Some(addr) = lookup((ns, sym, ty)) {
        return addr as Word;
    }
    match FACTORY.get() {
        // The session keeps its own entries (they run code it compiled), so
        // nothing is registered here.
        Some(make) => match make((ns, sym, ty)) {
            Ok(addr) => addr as Word,
            Err(msg) => raise(msg).into_word(),
        },
        None => fatal("rt_ffi_callback_address: no entry was registered for this callback at startup"),
    }
}

/// Registers an entry an executable carries: `args` is `[entry address,
/// function type, definition address, definition length]`, the definition
/// being the words `typelisp_mem::type_words::definition_words` writes, a
/// constant in the executable. Called by the generated `main` before
/// anything runs.
///
/// # Safety
///
/// The words must be valid; the last two must describe
/// The words of a definition, whose strings are constants.
#[no_mangle]
pub unsafe extern "C" fn rt_ffi_callback_register(entry: Word, ty: Word, words: Word, words_len: Word) -> Word {
    let ty = TypeId::from_word(ty)
        .unwrap_or_else(|| fatal("rt_ffi_callback_register: the function type is not a type"));
    let words = core::slice::from_raw_parts(words as usize as *const Word, words_len as usize);
    let (ns, sym) = typelisp_mem::type_words::read_definition(words);
    register((ns, sym, ty), entry as usize);
    0
}

/// The first thing an entry does: take this thread out of the native section
/// the C call put it in, so the callback may use the heap.
///
/// A callback can only be run where typelisp is waiting in a C call it made
/// itself. Called on another thread, or while this thread is running typelisp
/// rather than waiting in C (a signal handler, say), there is no heap to hand
/// it — and nothing to return an error to — so the process stops with the
/// reason.
///
/// # Safety
///
/// Called only from an entry, as its first action.
#[no_mangle]
pub unsafe extern "C" fn rt_ffi_callback_enter() -> Word {
    if !typelisp_abi::heap_registered() {
        fatal(
            "a C callback was called on a thread that is not running typelisp — a callback can \
             only be called while the C function typelisp called is running, on its thread",
        );
    }
    let heap = active_heap();
    if !heap.is_native() {
        fatal(
            "a C callback was called while typelisp was not waiting in a C call on this thread \
             — from a signal handler, or from a function C runs at exit (`atexit`), for example. \
             A callback can only be called while the C function typelisp called is running.",
        );
    }
    heap.leave_native();
    0
}

/// The last thing an entry does before returning to C: re-enter the native
/// section [`rt_ffi_callback_enter`] left.
///
/// # Safety
///
/// Called only from an entry, after [`rt_ffi_callback_enter`].
#[no_mangle]
pub unsafe extern "C" fn rt_ffi_callback_leave() -> Word {
    active_heap().enter_native();
    0
}

/// Runs the callback: `args` is `[function address, n, kind0, word0, aux0,
/// ..., kind(n-1), word(n-1), aux(n-1)]`, the function a coroutine-ABI body.
/// A kind of `0` is a word already in the function's own representation (a
/// value wider than a word is several of them, the low one first); `1`
/// is a C string's address, copied into a typelisp string here; `2` is a
/// typed pointer, whose `aux` is its pointee type (a `TypeId::word`) — it
/// has to point at such an object in memory an `unsafe`
/// allocated ([`crate::c_mem::check`]). `aux` is otherwise unused.
///
/// Answers the function's value, or `0` when it failed or an earlier
/// callback's failure is still waiting — see the module comment. Never
/// unwinds: it returns into C.
///
/// # Safety
///
/// `args` must describe a coroutine-ABI function taking exactly the `n`
/// words given, and run between [`rt_ffi_callback_enter`] and
/// [`rt_ffi_callback_leave`].
#[no_mangle]
pub unsafe extern "C" fn rt_ffi_callback_invoke(args: *const Word, argc: u32) -> i64 {
    if argc < 2 {
        fatal("rt_ffi_callback_invoke: expected the function and its argument count");
    }
    if PENDING.with(|p| p.borrow().is_some()) {
        return 0;
    }
    let f: crate::coroutine::CoroutineFn = core::mem::transmute(*args as usize);
    let n = *args.add(1) as usize;
    if argc as usize != 2 + 3 * n {
        fatal("rt_ffi_callback_invoke: the argument count does not match the words given");
    }
    let triples: Vec<(Word, Word, Word)> =
        (0..n).map(|i| (*args.add(2 + 3 * i), *args.add(3 + 3 * i), *args.add(4 + 3 * i))).collect();
    let run = || -> Result<i64, Raised> {
        let mut words = Vec::with_capacity(n);
        for (kind, word, aux) in &triples {
            words.push(match kind {
                0 => *word,
                // Allocating a string never collects, so the strings made
                // before the body starts need no roots of their own.
                1 => callback_string(*word)?,
                2 => {
                    let pointee = TypeId::from_word(*aux)
                        .unwrap_or_else(|| fatal("rt_ffi_callback_invoke: a typed pointer's pointee is not a type"));
                    match crate::c_mem::check(*word as usize, pointee) {
                        Ok(()) => *word,
                        Err(msg) => return Err(raise(msg)),
                    }
                }
                k => fatal(&format!("rt_ffi_callback_invoke: unknown argument kind {}", k)),
            });
        }
        crate::drive_to_completion(f, &words, &[], "a C callback")
    };
    match run() {
        Ok(v) => v,
        // Kept rather than left raised: what C calls next must not see it,
        // and nothing but the thunk around the C call may raise it again.
        Err(_raised) => {
            let payload = typelisp_abi::take_raised().unwrap_or_else(|| fatal("rt_ffi_callback_invoke: a raise with nothing raised"));
            PENDING.with(|p| *p.borrow_mut() = Some(payload));
            0
        }
    }
}

/// A C string a callback was handed, copied onto the heap as a tagged string.
/// Raises — inside the callback, where it is kept like any other failure —
/// if it is null or not UTF-8.
unsafe fn callback_string(p: Word) -> Result<Word, Raised> {
    if p == 0 {
        return Err(raise(
            "ffi: C passed a callback a null pointer where a `string` was declared. Declare the \
             parameter as `ptr` if it can be null."
                .to_string(),
        ));
    }
    let bytes = core::ffi::CStr::from_ptr(p as usize as *const core::ffi::c_char).to_bytes();
    match core::str::from_utf8(bytes) {
        Ok(s) => {
            let s = s.to_string();
            Ok(crate::encode(active_heap().alloc_string(s)))
        }
        Err(_) => Err(raise(
            "ffi: C passed a callback bytes that are not valid UTF-8 where a `string` was \
             declared. Declare the parameter as `ptr` to handle the bytes yourself."
                .to_string(),
        )),
    }
}
