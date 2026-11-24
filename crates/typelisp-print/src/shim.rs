//! The printing builtins as compiled code calls them: the pieces an
//! expanded `format`/`print`/`println` is made of (`rt_fmt_*`), `rt_pprint`
//! and the `pprint-*` operators.
//!
//! # Why these are here and not in `typelisp-rt`
//!
//! Every other `rt_*` shim lives in `typelisp-rt`. These cannot, and the
//! reason is the linker rather than taste. `compile-file` links exactly one
//! archive — `typelisp-rt`'s `staticlib` — and the linker pulls archive
//! *members*: an object is taken whole or not at all. A wrapper in
//! `typelisp-rt` would share an object with `rt_cons`, which every program
//! calls, so its reference to the directive engine would drag the engine into
//! every program. Defining them in this crate keeps the whole subsystem in
//! members of its own, pulled in exactly when something calls one.
//!
//! (`typelisp-rt` names this crate once, in a `pub use` that generates no
//! code, purely so rustc puts its objects in the archive at all. See that
//! line's comment.)
//!
//! # Representations
//!
//! [`crate::runtime::print_builtin`] speaks [`Value`], the way the
//! interpreter does. The compiled ABI does not — an `i64`, a `bool` and `()`
//! are bare machine words there, only heap values are tagged — so each shim
//! converts at its own edge, per the signature the checker gave it
//! (`typeck::format`/`check_print_like`/`check_pprint`, and
//! `registry`'s `pprint-*` entries).
//!
//! # Raising
//!
//! A bad `~` directive and a `print-object` method that panics are both
//! language-level failures: the shim raises ([`typelisp_abi::raise`]) and
//! returns, and the compiled code that called it leaves for the `catch`/
//! `unwind-protect` region that should see them.

use typelisp_mem::Word;
use typelisp_mem::prelude::*;
use typelisp_abi::{active_heap, decode, encode, fatal, raise, Raised};
use typelisp_mem::symbols::well_known_symbol;
use typelisp_mem::{wk, Value};

use crate::runtime::{print_builtin, PrintError};

/// Runs `name` with `args` already decoded, and converts the outcome to the
/// compiled ABI: [`PrintError::Raise`] is raised, [`PrintError::Shape`]
/// aborts. A shim that gets `Err` returns at once; the compiled code that
/// called it sees the raise.
///
/// When a `print-object` method or `~/name/` definition the run called
/// raised, that raise is the one kept: the printing error that stopped the
/// run only says it happened, and replacing the method's own `panic` or
/// `throw` with it would lose what the program raised.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
unsafe fn run(name: u32, args: Vec<Value>) -> Result<Value, Raised> {
    match print_builtin(active_heap(), well_known_symbol(name), &args) {
        Some(Ok(v)) => Ok(v),
        Some(Err(PrintError::Raise(msg))) => match typelisp_abi::raised() {
            Err(raised) => Err(raised),
            Ok(()) => Err(raise(msg)),
        },
        Some(Err(PrintError::Shape(msg))) => fatal(&msg),
        None => fatal(&format!("print shim: \"{}\" is not a printing builtin", well_known_symbol(name))),
    }
}

/// Aborts unless the call site passed at least `n` arguments.
///
/// Every shim below calls this once, before reading anything: the reads
/// themselves are unchecked, so the check has to cover all of them and
/// therefore has to come first.
fn check_argc(argc: u32, n: usize, who: &str) {
    if (argc as usize) < n {
        fatal(&format!("{}: expected {} arguments, got {}", who, n, argc));
    }
}

/// `pprint`/`pprint-fill`/`pprint-linear`/`pprint-tabular` — `which` names
/// which (as a string, so one shim serves all four), `value` is the object
/// and `colinc` is `pprint-tabular`'s column width as a bare `i64`.
///
/// # Safety
///
/// The words must be valid; a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint(which: Word, value: Word, colinc: Word) -> Word {
    let which = decode(which);
    let value = decode(value);
    let colinc = Value::Int(colinc as i64);
    if run(wk::F_PPRINT_RT, vec![which, value, colinc]).is_err() {
        return 0;
    }
    0
}

/// Opens a logical block: `obj` is the list `pprint-pop` walks,
/// `prefix`/`suffix` the prefix and suffix strings, `per_line` the
/// per-line-prefix flag as a bare `0`/`1`.
///
/// # Safety
///
/// The words must be valid; a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_block_start(obj: Word, prefix: Word, per_line: bool, suffix: Word) -> Word {
    let obj = decode(obj);
    let prefix = decode(prefix);
    let per_line = Value::Bool(per_line);
    let suffix = decode(suffix);
    if run(wk::F_PPRINT_BLOCK_START_RT, vec![obj, prefix, per_line, suffix]).is_err() {
        return 0;
    }
    0
}

/// Closes a logical block; closing the outermost one writes the session out.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_block_end() -> Word {
    if run(wk::F_PPRINT_BLOCK_END_RT, Vec::new()).is_err() {
        return 0;
    }
    0
}

/// `(pprint-newline :linear|:fill|:miser|:mandatory)`.
///
/// # Safety
///
/// The words must be valid; a `Heap` must be
/// registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_newline(x: Word) -> Word {
    if run(wk::F_PPRINT_NEWLINE, vec![decode(x)]).is_err() {
        return 0;
    }
    0
}

/// `(pprint-indent :block|:current n)` — `n` is a bare `i64`.
///
/// # Safety
///
/// The words must be valid; a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_indent(kind: Word, n: Word) -> Word {
    let kind = decode(kind);
    let n = Value::Int(n as i64);
    if run(wk::F_PPRINT_INDENT, vec![kind, n]).is_err() {
        return 0;
    }
    0
}

/// `(pprint-tab kind colnum colinc)` — `colnum`/`colinc` are bare `i64`s.
///
/// # Safety
///
/// The words must be valid; a
/// `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_tab(kind: Word, colnum: Word, colinc: Word) -> Word {
    let kind = decode(kind);
    let colnum = Value::Int(colnum as i64);
    let colinc = Value::Int(colinc as i64);
    if run(wk::F_PPRINT_TAB, vec![kind, colnum, colinc]).is_err() {
        return 0;
    }
    0
}

/// `(pprint-pop)` — the next element of the innermost open block's list, as a
/// tagged `Sexpr`.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_pop() -> Word {
    match run(wk::F_PPRINT_POP, Vec::new()) {
        Ok(v) => encode(v),
        Err(_) => 0,
    }
}

/// `(pprint-list-exhausted)` — a bare `0`/`1`.
///
/// # Safety
///
/// A `Heap` must be registered on this thread.
#[no_mangle]
pub unsafe extern "C-unwind" fn rt_pprint_list_exhausted() -> bool {
    match run(wk::F_PPRINT_LIST_EXHAUSTED, Vec::new()) {
        Ok(Value::Bool(b)) => b,
        Err(_) => false,
        other => fatal(&format!("rt_pprint_list_exhausted: expected a bool, got {:?}", other)),
    }
}

// ---- the pieces of an expanded `format` -------------------------------------
//
// One shim per piece (`crate::format::ops`), each calling its piece and
// nothing else, so an executable links the renderers its control strings
// use. Arguments arrive as the expansion passes them: the buffer, strings,
// `Sexpr`s and lists tagged; `i32`s and flags as bare words.

/// The arguments of one piece's call.
struct Args(*const Word);

impl Args {
    unsafe fn tagged(&self, i: usize) -> typelisp_mem::Value {
        decode(*self.0.add(i))
    }

    unsafe fn int(&self, i: usize) -> i64 {
        *self.0.add(i) as i64
    }

    unsafe fn flag(&self, i: usize) -> bool {
        *self.0.add(i) != 0
    }

    unsafe fn text(&self, i: usize) -> String {
        match self.tagged(i) {
            typelisp_mem::Value::Str(id) => active_heap().string(id).to_string(),
            other => fatal(&format!("format piece: argument {} is not a string: {:?}", i, other)),
        }
    }

    unsafe fn padding(&self, from: usize) -> crate::format::ops::Padding {
        crate::format::ops::Padding {
            padded: self.flag(from),
            mincol: self.int(from + 1),
            colinc: self.int(from + 2),
            minpad: self.int(from + 3),
            padchar: self.int(from + 4),
            at: self.flag(from + 5),
        }
    }

    unsafe fn integer_params(&self) -> crate::format::ops::IntegerParams {
        crate::format::ops::IntegerParams {
            radix: self.int(2),
            mincol: self.int(3),
            padchar: self.int(4),
            commachar: self.int(5),
            interval: self.int(6),
            colon: self.flag(7),
            at: self.flag(8),
        }
    }
}

/// A piece's outcome as the compiled ABI answers it: the word, or a raise
/// (the one a `print-object` method the piece called raised, when there is
/// one: replacing it with the printing error would lose what the program
/// raised).
fn answer<T: Default>(r: Result<T, String>) -> T {
    match r {
        Ok(w) => w,
        Err(msg) => match typelisp_abi::raised() {
            Err(raised) => raised.into_zero(),
            Ok(()) => raise(msg).into_zero(),
        },
    }
}

fn unit(r: Result<(), String>) -> Word {
    answer(r.map(|()| 0))
}

macro_rules! piece {
    ($shim:ident, $argc:expr, |$a:ident| $body:expr) => {
        piece!($shim, $argc, Word, |$a| $body);
    };
    ($shim:ident, $argc:expr, $ret:ty, |$a:ident| $body:expr) => {
        /// A piece of an expanded `format` (`crate::format::ops`).
        ///
        /// # Safety
        ///
        /// `args` must point to `argc` valid words in the representations
        /// the expansion passes; a `Heap` must be registered on this thread.
        #[no_mangle]
        pub unsafe extern "C-unwind" fn $shim(args: *const Word, argc: u32) -> $ret {
            check_argc(argc, $argc, stringify!($shim));
            let $a = Args(args);
            $body
        }
    };
}

use crate::format::ops;

piece!(rt_fmt_open, 0, |_a| answer(ops::open(active_heap()).map(encode)));
piece!(rt_fmt_text, 2, |a| unit(ops::text(active_heap(), a.tagged(0), &a.text(1))));
piece!(rt_fmt_fresh_line, 2, |a| unit(ops::fresh_line(active_heap(), a.tagged(0), a.int(1))));
piece!(rt_fmt_string, 9, |a| unit(ops::string(active_heap(), a.tagged(0), &a.text(1), a.flag(2), &a.padding(3))));
piece!(rt_fmt_number, 8, |a| unit(ops::number(active_heap(), a.tagged(0), a.tagged(1), &a.padding(2))));
piece!(rt_fmt_value, 9, |a| unit(ops::value(active_heap(), a.tagged(0), a.tagged(1), a.flag(2), &a.padding(3))));
piece!(rt_fmt_integer, 9, |a| unit(ops::integer(active_heap(), a.tagged(0), a.tagged(1), &a.integer_params())));
piece!(rt_fmt_integer_any, 9, |a| unit(ops::integer_any(active_heap(), a.tagged(0), a.tagged(1), &a.integer_params())));
piece!(rt_fmt_words, 4, |a| unit(ops::words(active_heap(), a.tagged(0), a.tagged(1), a.flag(2), a.flag(3))));
piece!(rt_fmt_plural, 3, |a| unit(ops::plural(active_heap(), a.tagged(0), a.tagged(1), a.flag(2))));
piece!(rt_fmt_char, 4, |a| unit(ops::character(active_heap(), a.tagged(0), a.tagged(1), a.flag(2), a.flag(3))));
piece!(rt_fmt_fixed, 8, |a| {
    let p = ops::FixedParams { w: a.int(2), d: a.int(3), k: a.int(4), overflow: a.int(5), padchar: a.int(6), at: a.flag(7) };
    unit(ops::fixed(active_heap(), a.tagged(0), a.tagged(1), &p))
});
piece!(rt_fmt_exponent, 7, |a| {
    let p = ops::ExponentParams { w: a.int(2), d: a.int(3), exptchar: a.int(4), padchar: a.int(5), at: a.flag(6) };
    unit(ops::exponent(active_heap(), a.tagged(0), a.tagged(1), &p))
});
piece!(rt_fmt_general, 3, |a| unit(ops::general(active_heap(), a.tagged(0), a.tagged(1), a.flag(2))));
piece!(rt_fmt_dollars, 8, |a| {
    let p = ops::DollarParams { d: a.int(2), n: a.int(3), w: a.int(4), padchar: a.int(5), colon: a.flag(6), at: a.flag(7) };
    unit(ops::dollars(active_heap(), a.tagged(0), a.tagged(1), &p))
});
piece!(rt_fmt_tab, 5, |a| unit(ops::tab(active_heap(), a.tagged(0), a.int(1), a.int(2), a.flag(3), a.flag(4))));
piece!(rt_fmt_newline, 2, |a| unit(ops::newline(active_heap(), a.tagged(0), a.int(1))));
piece!(rt_fmt_indent, 3, |a| unit(ops::indent(active_heap(), a.tagged(0), a.flag(1), a.int(2))));
piece!(rt_fmt_case_open, 1, |a| unit(ops::case_open(active_heap(), a.tagged(0))));
piece!(rt_fmt_case_close, 3, |a| unit(ops::case_close(active_heap(), a.tagged(0), a.flag(1), a.flag(2))));
piece!(rt_fmt_just_open, 1, |a| unit(ops::just_open(active_heap(), a.tagged(0))));
piece!(rt_fmt_just_segment, 1, |a| unit(ops::just_segment(active_heap(), a.tagged(0))));
piece!(rt_fmt_just_close, 7, |a| {
    let p = ops::JustParams { mincol: a.int(1), colinc: a.int(2), minpad: a.int(3), padchar: a.int(4), colon: a.flag(5), at: a.flag(6) };
    unit(ops::just_close(active_heap(), a.tagged(0), &p))
});
piece!(rt_fmt_block_open, 4, |a| unit(ops::block_open(active_heap(), a.tagged(0), &a.text(1), a.flag(2), &a.text(3))));
piece!(rt_fmt_block_close, 2, |a| unit(ops::block_close(active_heap(), a.tagged(0), &a.text(1))));
piece!(rt_fmt_result, 2, |a| {
    let heap = active_heap();
    answer(ops::result(heap, a.tagged(0), a.flag(1)).map(|s| encode(heap.alloc_string(s))))
});
piece!(rt_fmt_emit, 2, |a| unit(ops::emit_out(active_heap(), a.tagged(0), a.flag(1))));
piece!(rt_fmt_first, 1, |a| answer(ops::first(active_heap(), a.tagged(0)).map(encode)));
piece!(rt_fmt_rest, 1, |a| answer(ops::rest(active_heap(), a.tagged(0)).map(encode)));
piece!(rt_fmt_empty, 1, bool, |a| ops::empty(a.tagged(0)));
piece!(rt_fmt_length, 2, i32, |a| answer(ops::length(active_heap(), a.tagged(0), &a.text(1)).map(|n| n as i32)));
piece!(rt_fmt_goto, 3, |a| answer(ops::goto(active_heap(), a.tagged(0), a.int(1), a.flag(2)).map(encode)));
piece!(rt_fmt_as_list, 2, |a| answer(ops::as_list(active_heap(), a.tagged(0), &a.text(1)).map(encode)));
piece!(rt_fmt_param, 3, i32, |a| answer(ops::param(active_heap(), a.tagged(0), a.int(1), a.int(2)).map(|n| n as i32)));
piece!(rt_fmt_select, 1, i32, |a| answer(ops::select(active_heap(), a.tagged(0)).map(|n| n as i32)));
piece!(rt_fmt_bool, 1, bool, |a| answer(ops::boolean(a.tagged(0))));
