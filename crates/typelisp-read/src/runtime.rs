//! What the reader needs from the environment around it, for the two entry
//! points that are *builtins* rather than driver loops.
//!
//! A driver (`project::Loader`, the REPL, `compile::aot`) hands the reader a
//! [`ReadEval`] per call, because it has both halves of one in its hand
//! already. The `read`/`read-datum-at` builtins do not: they are reached
//! from inside an evaluation, interpreted through `Interp::eval_builtin` and
//! compiled through `rt_read`, and the compiled path has no `Interp` to pass
//! even in principle. So the interpreter registers itself once, here, and
//! [`HookEval`] turns that registration back into a `ReadEval`.
//!
//! Plain `fn` pointers reaching the running interpreter through a
//! thread-local of its own, and thread-local for one program per thread —
//! the arrangement `typelisp_print::runtime::PrintHooks` already has, and
//! for the same two reasons.

use std::cell::Cell;

use typelisp_mem::{Heap, Value};

use crate::reader::ReadEval;

/// The evaluator, as two function pointers.
#[derive(Clone, Copy)]
pub struct ReadHooks {
    /// `#.`: check and run the datum just read. See [`ReadEval::read_eval`].
    pub read_eval: fn(&mut Heap, Value) -> Result<Value, String>,
    /// A macro character's function, on the text after it. See
    /// [`ReadEval::call_reader_macro`].
    pub call_reader_macro: fn(&mut Heap, Value, char, &str) -> Result<(Value, usize), String>,
}

thread_local! {
    static HOOKS: Cell<Option<ReadHooks>> = const { Cell::new(None) };
}

/// Register (or, with `None`, withdraw) this thread's evaluator.
pub fn set_read_hooks(h: Option<ReadHooks>) {
    HOOKS.with(|c| c.set(h));
}

/// This thread's registered evaluator, if any.
pub fn read_hooks() -> Option<ReadHooks> {
    HOOKS.with(|c| c.get())
}

/// A [`ReadEval`] that forwards to whatever [`set_read_hooks`] last
/// registered.
///
/// Read fresh per call rather than captured at construction, for the reason
/// `with_active_interp` gives: a nested interpreter (`compile-file` builds
/// its own) would otherwise leave this pointing at a finished one.
pub struct HookEval;

impl ReadEval for HookEval {
    fn read_eval(&self, heap: &mut Heap, form: Value) -> Result<Value, String> {
        match read_hooks() {
            Some(h) => (h.read_eval)(heap, form),
            None => Err("`#.` needs an evaluator, and none is registered on this thread".to_string()),
        }
    }

    fn call_reader_macro(
        &self,
        heap: &mut Heap,
        f: Value,
        ch: char,
        rest: &str,
    ) -> Result<(Value, usize), String> {
        match read_hooks() {
            Some(h) => (h.call_reader_macro)(heap, f, ch, rest),
            None => Err(format!("`{}` is a reader macro, and no evaluator is registered on this thread", ch)),
        }
    }
}
